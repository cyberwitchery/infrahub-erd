//! dot output
//!
//! renders a parsed schema as a graphviz dot diagram.

use crate::dedup::{EdgeSide, MergedEdge};
use crate::error;
use crate::parse::{Cardinality, Schema};
use crate::render::{render_document, Renderer};
use std::fmt::Write;

/// escape a string for use inside a dot record label.
///
/// record labels use `{`, `}`, `|`, and `<`, `>` as structural delimiters,
/// and the enclosing attribute value uses `"` and `\`. all of these must be
/// backslash-escaped when they appear in literal text.
fn escape_dot_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' | '\\' | '{' | '}' | '|' | '<' | '>' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// render a schema as a graphviz dot string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    render_document(&DotRenderer, schema, show_attributes)
}

/// graphviz dot emitter: record-shaped nodes with `\l`-joined attribute rows
/// and crow's-foot cardinality on directed edges.
struct DotRenderer;

impl Renderer for DotRenderer {
    fn document_header(&self, out: &mut String) -> std::fmt::Result {
        writeln!(out, "digraph schema {{")?;
        writeln!(out, "  rankdir=LR;")?;
        writeln!(
            out,
            "  node [shape=record, fontname=\"Helvetica\", fontsize=11];"
        )?;
        writeln!(out, "  edge [fontname=\"Helvetica\", fontsize=9];")
    }

    fn document_footer(&self, out: &mut String) -> std::fmt::Result {
        writeln!(out, "}}")
    }

    fn entity_open(&self, out: &mut String, name: &str, with_attributes: bool) -> std::fmt::Result {
        let esc_name = escape_dot_label(name);
        if with_attributes {
            write!(out, "  \"{}\" [label=\"{{{}|", esc_name, esc_name)
        } else {
            writeln!(out, "  \"{}\" [label=\"{}\"];", esc_name, esc_name)
        }
    }

    fn attribute_line(&self, out: &mut String, name: &str, type_display: &str) -> std::fmt::Result {
        write!(
            out,
            "{}: {}\\l",
            escape_dot_label(name),
            escape_dot_label(type_display)
        )
    }

    fn entity_close(&self, out: &mut String, with_attributes: bool) -> std::fmt::Result {
        if with_attributes {
            writeln!(out, "}}\"];")
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
        let arrowhead = match edge.left_to_right.cardinality {
            Cardinality::One => "normal",
            Cardinality::Many => "crow",
        };
        let arrowtail = match rev.cardinality {
            Cardinality::One => "normal",
            Cardinality::Many => "crow",
        };
        writeln!(
            out,
            "  \"{}\" -> \"{}\" [taillabel=\"{}\", headlabel=\"{}\", arrowhead={}, arrowtail={}, dir=both];",
            escape_dot_label(&edge.left),
            escape_dot_label(&edge.right),
            escape_dot_label(&edge.left_to_right.field_name),
            escape_dot_label(&rev.field_name),
            arrowhead,
            arrowtail,
        )
    }

    fn edge_unidirectional(&self, out: &mut String, edge: &MergedEdge) -> std::fmt::Result {
        let arrowhead = match edge.left_to_right.cardinality {
            Cardinality::One => "",
            Cardinality::Many => ", arrowhead=crow",
        };
        writeln!(
            out,
            "  \"{}\" -> \"{}\" [label=\"{}\"{}];",
            escape_dot_label(&edge.left),
            escape_dot_label(&edge.right),
            escape_dot_label(&edge.left_to_right.field_name),
            arrowhead
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Entity, Relationship};
    use crate::render::test_helpers::{enum_schema, generic_schema, self_ref_schema, test_schema};

    #[test]
    fn test_render_with_attributes() {
        let dot = render(&test_schema(), true).unwrap();
        assert!(dot.contains("digraph schema {"));
        assert!(dot.contains("rankdir=LR"));
        assert!(dot.contains("\"InfraDevice\" [label=\"{InfraDevice|name: TextAttribute\\l}\"]"));
        assert!(dot.contains("\"InfraInterface\" [label=\"InfraInterface\"]"));
        // bidirectional edge merged: headlabel/taillabel instead of separate edges
        assert!(dot.contains(
            "\"InfraDevice\" -> \"InfraInterface\" [taillabel=\"interfaces\", headlabel=\"device\", arrowhead=crow, arrowtail=normal, dir=both]"
        ));
        // unidirectional edge to non-entity target unchanged
        assert!(dot.contains("\"InfraDevice\" -> \"LocationSite\" [label=\"site\"]"));
        // reverse edge no longer rendered separately
        assert!(!dot.contains("\"InfraInterface\" -> \"InfraDevice\""));
    }

    #[test]
    fn test_render_without_attributes() {
        let dot = render(&test_schema(), false).unwrap();
        assert!(dot.contains("\"InfraDevice\" [label=\"InfraDevice\"]"));
        assert!(!dot.contains("TextAttribute"));
    }

    #[test]
    fn test_render_enum_attribute() {
        let dot = render(&enum_schema(), true).unwrap();
        // enum-typed attribute lists its allowed values in order
        assert!(dot.contains("status: Status(ACTIVE,INACTIVE)"));
        // non-enum attribute is unchanged: bare type, no value list
        assert!(dot.contains("hostname: TextAttribute"));
        assert!(!dot.contains("TextAttribute("));
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema {
            entities: vec![],
            enums: vec![],
        };
        let dot = render(&schema, true).unwrap();
        assert!(dot.contains("digraph schema {"));
        assert!(dot.contains("}"));
    }

    #[test]
    fn test_escape_dot_label() {
        assert_eq!(escape_dot_label("plain"), "plain");
        assert_eq!(escape_dot_label(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_dot_label("a{b}c"), r"a\{b\}c");
        assert_eq!(escape_dot_label("a|b"), r"a\|b");
        assert_eq!(escape_dot_label("a<b>c"), r"a\<b\>c");
        assert_eq!(escape_dot_label(r"a\b"), r"a\\b");
    }

    #[test]
    fn test_render_special_chars_in_names() {
        let schema = Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "My|Entity".to_string(),
                attributes: vec![Attribute {
                    name: "field{x}".to_string(),
                    type_name: "Type<T>".to_string(),
                }],
                relationships: vec![Relationship {
                    field_name: "ref\"edge".to_string(),
                    target: "Other|Node".to_string(),
                    cardinality: Cardinality::One,
                }],
            }],
        };

        let dot = render(&schema, true).unwrap();
        // entity name escaped in node id and label
        assert!(dot.contains(r#""My\|Entity" [label="{My\|Entity|field\{x\}: Type\<T\>\l}"]"#));
        // edge label and target escaped
        assert!(dot.contains(r#""My\|Entity" -> "Other\|Node" [label="ref\"edge"]"#));
    }

    #[test]
    fn test_render_self_referential_edge() {
        let dot = render(&self_ref_schema(), false).unwrap();
        // self-loop edge: left == right
        assert!(dot.contains(r#""Tree" -> "Tree" [label="parent"]"#));
    }

    #[test]
    fn test_render_generic_entities() {
        let dot = render(&generic_schema(), true).unwrap();
        // an interface-derived entity is a node like any other
        assert!(dot.contains(r#""CoreGroup" [label="{CoreGroup|group_type: TextAttribute\l}"]"#));
        assert!(dot.contains(r#""CoreNode" [label="CoreNode"]"#));
        // concrete -> generic and generic -> generic both render
        assert!(dot.contains(r#""InfraDevice" -> "CoreGroup" [label="primary_group"]"#));
        assert!(dot.contains(r#""CoreGroup" -> "CoreNode" [label="members", arrowhead=crow]"#));
    }
}
