//! dot output
//!
//! renders a parsed schema as a graphviz dot diagram.

use crate::parse::{Cardinality, Schema};
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
pub fn render(schema: &Schema, show_attributes: bool) -> String {
    let mut out = String::new();
    writeln!(out, "digraph schema {{").unwrap();
    writeln!(out, "  rankdir=LR;").unwrap();
    writeln!(
        out,
        "  node [shape=record, fontname=\"Helvetica\", fontsize=11];"
    )
    .unwrap();
    writeln!(out, "  edge [fontname=\"Helvetica\", fontsize=9];").unwrap();

    for entity in &schema.entities {
        let esc_name = escape_dot_label(&entity.name);
        if show_attributes && !entity.attributes.is_empty() {
            let attrs: String = entity
                .attributes
                .iter()
                .map(|a| {
                    format!(
                        "{}: {}",
                        escape_dot_label(&a.name),
                        escape_dot_label(&a.type_name)
                    )
                })
                .collect::<Vec<_>>()
                .join("\\l");
            writeln!(
                out,
                "  \"{}\" [label=\"{{{}|{}\\l}}\"];",
                esc_name, esc_name, attrs
            )
            .unwrap();
        } else {
            writeln!(out, "  \"{}\" [label=\"{}\"];", esc_name, esc_name).unwrap();
        }
    }

    for entity in &schema.entities {
        for rel in &entity.relationships {
            let arrowhead = match rel.cardinality {
                Cardinality::One => "",
                Cardinality::Many => ", arrowhead=crow",
            };
            writeln!(
                out,
                "  \"{}\" -> \"{}\" [label=\"{}\"{}];",
                escape_dot_label(&entity.name),
                escape_dot_label(&rel.target),
                escape_dot_label(&rel.field_name),
                arrowhead
            )
            .unwrap();
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Entity, Relationship};

    fn test_schema() -> Schema {
        Schema {
            entities: vec![
                Entity {
                    name: "InfraDevice".to_string(),
                    attributes: vec![Attribute {
                        name: "name".to_string(),
                        type_name: "TextAttribute".to_string(),
                    }],
                    relationships: vec![
                        Relationship {
                            field_name: "interfaces".to_string(),
                            target: "InfraInterface".to_string(),
                            cardinality: Cardinality::Many,
                        },
                        Relationship {
                            field_name: "site".to_string(),
                            target: "LocationSite".to_string(),
                            cardinality: Cardinality::One,
                        },
                    ],
                },
                Entity {
                    name: "InfraInterface".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "device".to_string(),
                        target: "InfraDevice".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_render_with_attributes() {
        let dot = render(&test_schema(), true);
        assert!(dot.contains("digraph schema {"));
        assert!(dot.contains("rankdir=LR"));
        assert!(dot.contains("\"InfraDevice\" [label=\"{InfraDevice|name: TextAttribute\\l}\"]"));
        assert!(dot.contains("\"InfraInterface\" [label=\"InfraInterface\"]"));
        assert!(dot.contains(
            "\"InfraDevice\" -> \"InfraInterface\" [label=\"interfaces\", arrowhead=crow]"
        ));
        assert!(dot.contains("\"InfraDevice\" -> \"LocationSite\" [label=\"site\"]"));
        assert!(dot.contains("\"InfraInterface\" -> \"InfraDevice\" [label=\"device\"]"));
    }

    #[test]
    fn test_render_without_attributes() {
        let dot = render(&test_schema(), false);
        assert!(dot.contains("\"InfraDevice\" [label=\"InfraDevice\"]"));
        assert!(!dot.contains("TextAttribute"));
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema { entities: vec![] };
        let dot = render(&schema, true);
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

        let dot = render(&schema, true);
        // entity name escaped in node id and label
        assert!(dot.contains(r#""My\|Entity" [label="{My\|Entity|field\{x\}: Type\<T\>\l}"]"#));
        // edge label and target escaped
        assert!(dot.contains(r#""My\|Entity" -> "Other\|Node" [label="ref\"edge"]"#));
    }
}
