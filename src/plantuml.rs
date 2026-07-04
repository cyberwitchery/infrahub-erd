//! plantuml output
//!
//! renders a parsed schema as a plantuml er diagram.

use crate::dedup;
use crate::error;
use crate::parse::Schema;
use crate::render::{attribute_type_display, escape_attr, escape_quotes};
use std::fmt::Write;

/// render a schema as a plantuml er diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    let mut out = String::new();
    writeln!(out, "@startuml")?;

    for entity in &schema.entities {
        let name = escape_quotes(&entity.name);
        if show_attributes && !entity.attributes.is_empty() {
            writeln!(out, "    entity \"{}\" {{", name)?;
            for attr in &entity.attributes {
                writeln!(
                    out,
                    "        {} : {}",
                    escape_attr(&attr.name),
                    escape_attr(&attribute_type_display(schema, &attr.type_name))
                )?;
            }
            writeln!(out, "    }}")?;
        } else {
            writeln!(out, "    entity \"{}\" {{}}", name)?;
        }
    }

    let edges = dedup::deduplicate(schema);
    crate::render::render_edges(&mut out, &edges)?;

    writeln!(out, "@enduml")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};
    use crate::render::test_helpers::{enum_schema, test_schema};

    #[test]
    fn test_render_with_attributes() {
        let puml = render(&test_schema(), true).unwrap();
        assert!(puml.starts_with("@startuml\n"));
        assert!(puml.ends_with("@enduml\n"));
        assert!(puml.contains("entity \"InfraDevice\" {"));
        assert!(puml.contains("        name : TextAttribute"));
        assert!(puml.contains("entity \"InfraInterface\" {}"));
        // bidirectional edge merged with combined label
        assert!(
            puml.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces / device\"")
        );
        // unidirectional edge unchanged
        assert!(puml.contains("\"InfraDevice\" ||--|| \"LocationSite\" : \"site\""));
        // reverse edge no longer rendered separately
        assert!(!puml.contains("\"InfraInterface\" ||--|| \"InfraDevice\""));
    }

    #[test]
    fn test_render_without_attributes() {
        let puml = render(&test_schema(), false).unwrap();
        assert!(puml.contains("entity \"InfraDevice\" {}"));
        assert!(!puml.contains("TextAttribute"));
        assert!(
            puml.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces / device\"")
        );
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema {
            entities: vec![],
            enums: vec![],
        };
        let puml = render(&schema, true).unwrap();
        assert_eq!(puml, "@startuml\n@enduml\n");
    }

    #[test]
    fn test_render_enum_attribute() {
        let puml = render(&enum_schema(), true).unwrap();
        // enum-typed attribute lists its allowed values in order
        assert!(puml.contains("status : Status(ACTIVE,INACTIVE)"));
        // non-enum attribute is unchanged: bare type, no value list
        assert!(puml.contains("hostname : TextAttribute"));
        assert!(!puml.contains("TextAttribute("));
    }

    #[test]
    fn test_escape_plantuml() {
        assert_eq!(escape_quotes("plain"), "plain");
        assert_eq!(escape_quotes(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_quotes(r#""""#), "''");
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
        let puml = render(&schema, true).unwrap();
        assert!(puml.contains("entity \"My Entity\" {"));
        assert!(puml.contains("\"My Entity\" ||--|| \"Other Entity\" : \"a 'rel'\""));
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
                        name: "tag}val".to_string(),
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
        let puml = render(&schema, true).unwrap();
        // braces in type_name are replaced to avoid closing the entity block
        assert!(puml.contains("        path : Set(String)"));
        // brace in attr_name is replaced
        assert!(puml.contains("        tag)val : Text|Blob"));
        // colons and pipes pass through in quoted edge labels
        assert!(puml.contains(r#""A" ||--o{ "B" : "ref:link / back|ref""#));
    }
}
