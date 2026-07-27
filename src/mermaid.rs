//! mermaid output
//!
//! renders a parsed schema as a mermaid er diagram.

use crate::dedup::{EdgeSide, MergedEdge};
use crate::error;
use crate::parse::Schema;
use crate::render::{
    crowsfoot_edge_bidirectional, crowsfoot_edge_unidirectional, escape_attr, escape_quotes,
    render_document, Renderer,
};
use std::fmt::Write;

/// sanitize a string for use in mermaid attribute positions.
///
/// mermaid attribute fields are whitespace-delimited and unquoted, so spaces
/// are replaced with underscores in addition to the shared attribute escaping.
fn sanitize_word(s: &str) -> String {
    escape_attr(s).replace(' ', "_")
}

/// render a schema as a mermaid er diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    render_document(&MermaidRenderer, schema, show_attributes)
}

/// mermaid er-diagram emitter: quoted entity blocks with whitespace-delimited
/// `type name` attribute rows and shared crow's-foot edges.
struct MermaidRenderer;

impl Renderer for MermaidRenderer {
    fn document_header(&self, out: &mut String) -> std::fmt::Result {
        writeln!(out, "erDiagram")
    }

    fn entity_open(&self, out: &mut String, name: &str, with_attributes: bool) -> std::fmt::Result {
        if with_attributes {
            writeln!(out, "    \"{}\" {{", escape_quotes(name))
        } else {
            writeln!(out, "    \"{}\" {{}}", escape_quotes(name))
        }
    }

    fn attribute_line(&self, out: &mut String, name: &str, type_display: &str) -> std::fmt::Result {
        writeln!(
            out,
            "        {} {}",
            sanitize_word(type_display),
            sanitize_word(name)
        )
    }

    fn entity_close(&self, out: &mut String, with_attributes: bool) -> std::fmt::Result {
        if with_attributes {
            writeln!(out, "    }}")
        } else {
            Ok(())
        }
    }

    fn edge_bidirectional(
        &self,
        out: &mut String,
        edge: &MergedEdge,
        rev: &EdgeSide,
    ) -> std::fmt::Result {
        crowsfoot_edge_bidirectional(out, edge, rev)
    }

    fn edge_unidirectional(&self, out: &mut String, edge: &MergedEdge) -> std::fmt::Result {
        crowsfoot_edge_unidirectional(out, edge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};
    use crate::render::test_helpers::{enum_schema, generic_schema, self_ref_schema, test_schema};

    #[test]
    fn test_render_with_attributes() {
        let mermaid = render(&test_schema(), true).unwrap();
        assert!(mermaid.starts_with("erDiagram\n"));
        assert!(mermaid.contains("\"InfraDevice\" {"));
        assert!(mermaid.contains("        TextAttribute name"));
        assert!(mermaid.contains("\"InfraInterface\" {}"));
        // bidirectional edge merged with combined label
        assert!(
            mermaid.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces / device\"")
        );
        // unidirectional edge unchanged
        assert!(mermaid.contains("\"InfraDevice\" ||--|| \"LocationSite\" : \"site\""));
        // reverse edge no longer rendered separately
        assert!(!mermaid.contains("\"InfraInterface\" ||--|| \"InfraDevice\""));
    }

    #[test]
    fn test_render_without_attributes() {
        let mermaid = render(&test_schema(), false).unwrap();
        assert!(mermaid.contains("\"InfraDevice\" {}"));
        assert!(mermaid.contains("\"InfraInterface\" {}"));
        assert!(!mermaid.contains("TextAttribute"));
        assert!(
            mermaid.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces / device\"")
        );
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema {
            entities: vec![],
            enums: vec![],
        };
        let mermaid = render(&schema, true).unwrap();
        assert_eq!(mermaid, "erDiagram\n");
    }

    #[test]
    fn test_render_enum_attribute() {
        let mermaid = render(&enum_schema(), true).unwrap();
        // enum-typed attribute lists its allowed values in order
        assert!(mermaid.contains("Status(ACTIVE,INACTIVE) status"));
        // non-enum attribute is unchanged: bare type, no value list
        assert!(mermaid.contains("TextAttribute hostname"));
        assert!(!mermaid.contains("TextAttribute("));
    }

    #[test]
    fn test_escape_mermaid() {
        assert_eq!(escape_quotes("plain"), "plain");
        assert_eq!(escape_quotes(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_quotes(r#""""#), "''");
    }

    #[test]
    fn test_render_isolated_entity() {
        let schema = Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "Standalone".to_string(),
                attributes: vec![],
                relationships: vec![],
            }],
        };
        let mermaid = render(&schema, true).unwrap();
        assert!(mermaid.contains("\"Standalone\" {}"));
    }

    #[test]
    fn test_render_special_chars_in_names() {
        let schema = Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "My Entity".to_string(),
                attributes: vec![Attribute {
                    name: "full name".to_string(),
                    type_name: "Text".to_string(),
                }],
                relationships: vec![Relationship {
                    field_name: "a \"rel\"".to_string(),
                    target: "Other Entity".to_string(),
                    cardinality: Cardinality::One,
                }],
            }],
        };
        let mermaid = render(&schema, true).unwrap();
        assert!(mermaid.contains("\"My Entity\" {"));
        // attribute name with space is sanitized to underscore
        assert!(mermaid.contains("        Text full_name"));
        assert!(mermaid.contains("\"My Entity\" ||--|| \"Other Entity\" : \"a 'rel'\""));
    }

    #[test]
    fn test_render_special_chars_in_edge_labels_and_type() {
        let schema = Schema {
            enums: vec![],
            entities: vec![
                Entity {
                    name: "A".to_string(),
                    attributes: vec![Attribute {
                        name: "path".to_string(),
                        type_name: "Set{String}".to_string(),
                    }],
                    relationships: vec![Relationship {
                        field_name: "ref:link".to_string(),
                        target: "B".to_string(),
                        cardinality: Cardinality::Many,
                    }],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![Attribute {
                        name: "tag val".to_string(),
                        type_name: "Text|Blob".to_string(),
                    }],
                    relationships: vec![Relationship {
                        field_name: "back|ref".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        };
        let mermaid = render(&schema, true).unwrap();
        // braces in type_name are replaced to avoid breaking block syntax
        assert!(mermaid.contains("        Set(String) path"));
        // spaces in attr_name are replaced with underscores; pipe in type is preserved
        assert!(mermaid.contains("        Text|Blob tag_val"));
        // colons and pipes pass through in quoted edge labels
        assert!(mermaid.contains(r#""A" ||--o{ "B" : "ref:link / back|ref""#));
    }

    #[test]
    fn test_render_self_referential_edge() {
        let mermaid = render(&self_ref_schema(), false).unwrap();
        // self-loop edge: left == right
        assert!(mermaid.contains(r#""Tree" ||--|| "Tree" : "parent""#));
    }

    #[test]
    fn test_render_generic_entities() {
        let mermaid = render(&generic_schema(), true).unwrap();
        // an interface-derived entity is a node like any other
        assert!(mermaid.contains("\"CoreGroup\" {"));
        assert!(mermaid.contains("TextAttribute group_type"));
        assert!(mermaid.contains("\"CoreNode\" {}"));
        // concrete -> generic and generic -> generic both render
        assert!(mermaid.contains(r#""InfraDevice" ||--|| "CoreGroup" : "primary_group""#));
        assert!(mermaid.contains(r#""CoreGroup" ||--o{ "CoreNode" : "members""#));
    }
}
