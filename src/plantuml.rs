//! plantuml output
//!
//! renders a parsed schema as a plantuml er diagram.

use crate::dedup;
use crate::error;
use crate::parse::{Cardinality, Schema};
use std::fmt::Write;

/// escape a string for use in a plantuml er diagram.
///
/// plantuml uses `"` to quote entity names and to delimit relationship labels.
/// double quotes inside these strings break the parser.  plantuml has no
/// backslash-escape mechanism for this context, so the only safe option is to
/// replace `"` with `'`.
fn escape_plantuml(s: &str) -> String {
    s.replace('"', "'")
}

/// render a schema as a plantuml er diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    let mut out = String::new();
    writeln!(out, "@startuml")?;

    for entity in &schema.entities {
        let name = escape_plantuml(&entity.name);
        if show_attributes && !entity.attributes.is_empty() {
            writeln!(out, "    entity \"{}\" {{", name)?;
            for attr in &entity.attributes {
                writeln!(
                    out,
                    "        {} : {}",
                    escape_plantuml(&attr.name),
                    escape_plantuml(&attr.type_name)
                )?;
            }
            writeln!(out, "    }}")?;
        } else {
            writeln!(out, "    entity \"{}\" {{}}", name)?;
        }
    }

    let edges = dedup::deduplicate(schema);
    for edge in &edges {
        if let Some(ref rev) = edge.right_to_left {
            let left_card = match rev.cardinality {
                Cardinality::One => "||",
                Cardinality::Many => "}o",
            };
            let right_card = match edge.left_to_right.cardinality {
                Cardinality::One => "||",
                Cardinality::Many => "o{",
            };
            writeln!(
                out,
                "    \"{}\" {}--{} \"{}\" : \"{} / {}\"",
                escape_plantuml(&edge.left),
                left_card,
                right_card,
                escape_plantuml(&edge.right),
                escape_plantuml(&edge.left_to_right.field_name),
                escape_plantuml(&rev.field_name)
            )?;
        } else {
            let cardinality = match edge.left_to_right.cardinality {
                Cardinality::One => "||--||",
                Cardinality::Many => "||--o{",
            };
            writeln!(
                out,
                "    \"{}\" {} \"{}\" : \"{}\"",
                escape_plantuml(&edge.left),
                cardinality,
                escape_plantuml(&edge.right),
                escape_plantuml(&edge.left_to_right.field_name)
            )?;
        }
    }

    writeln!(out, "@enduml")?;
    Ok(out)
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
        let schema = Schema { entities: vec![] };
        let puml = render(&schema, true).unwrap();
        assert_eq!(puml, "@startuml\n@enduml\n");
    }

    #[test]
    fn test_escape_plantuml() {
        assert_eq!(escape_plantuml("plain"), "plain");
        assert_eq!(escape_plantuml(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_plantuml(r#""""#), "''");
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
        let puml = render(&schema, true).unwrap();
        assert!(puml.contains("entity \"My Entity\" {"));
        assert!(puml.contains("\"My Entity\" ||--|| \"Other Entity\" : \"a 'rel'\""));
    }
}
