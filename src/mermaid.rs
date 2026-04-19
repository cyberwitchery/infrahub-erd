//! mermaid output
//!
//! renders a parsed schema as a mermaid er diagram.

use crate::parse::{Cardinality, Schema};
use std::fmt::Write;

/// escape a string for use in a mermaid er diagram.
///
/// mermaid er diagrams use `"` to quote entity names (so names with spaces
/// work) and to delimit relationship labels.  double quotes inside these
/// strings break the parser.  mermaid has no backslash-escape mechanism, so
/// the only safe option is to replace `"` with `'`.
fn escape_mermaid(s: &str) -> String {
    s.replace('"', "'")
}

/// render a schema as a mermaid er diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> String {
    let mut out = String::new();
    writeln!(out, "erDiagram").unwrap();

    for entity in &schema.entities {
        if show_attributes && !entity.attributes.is_empty() {
            let name = escape_mermaid(&entity.name);
            writeln!(out, "    \"{}\" {{", name).unwrap();
            for attr in &entity.attributes {
                writeln!(
                    out,
                    "        {} {}",
                    escape_mermaid(&attr.type_name),
                    escape_mermaid(&attr.name)
                )
                .unwrap();
            }
            writeln!(out, "    }}").unwrap();
        }
    }

    for entity in &schema.entities {
        for rel in &entity.relationships {
            let cardinality = match rel.cardinality {
                Cardinality::One => "||--||",
                Cardinality::Many => "||--o{",
            };
            writeln!(
                out,
                "    \"{}\" {} \"{}\" : \"{}\"",
                escape_mermaid(&entity.name),
                cardinality,
                escape_mermaid(&rel.target),
                escape_mermaid(&rel.field_name)
            )
            .unwrap();
        }
    }

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
        let mermaid = render(&test_schema(), true);
        assert!(mermaid.starts_with("erDiagram\n"));
        assert!(mermaid.contains("\"InfraDevice\" {"));
        assert!(mermaid.contains("        TextAttribute name"));
        assert!(!mermaid.contains("\"InfraInterface\" {"));
        assert!(mermaid.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces\""));
        assert!(mermaid.contains("\"InfraDevice\" ||--|| \"LocationSite\" : \"site\""));
        assert!(mermaid.contains("\"InfraInterface\" ||--|| \"InfraDevice\" : \"device\""));
    }

    #[test]
    fn test_render_without_attributes() {
        let mermaid = render(&test_schema(), false);
        assert!(!mermaid.contains("\"InfraDevice\" {"));
        assert!(!mermaid.contains("TextAttribute"));
        assert!(mermaid.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces\""));
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema { entities: vec![] };
        let mermaid = render(&schema, true);
        assert_eq!(mermaid, "erDiagram\n");
    }

    #[test]
    fn test_escape_mermaid() {
        assert_eq!(escape_mermaid("plain"), "plain");
        assert_eq!(escape_mermaid(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_mermaid(r#""""#), "''");
    }

    #[test]
    fn test_render_special_chars_in_names() {
        let schema = Schema {
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
        let mermaid = render(&schema, true);
        assert!(mermaid.contains("\"My Entity\" {"));
        assert!(mermaid.contains("\"My Entity\" ||--|| \"Other Entity\" : \"a 'rel'\""));
    }
}
