//! shared rendering utilities
//!
//! common functions used by multiple diagram format renderers.

use crate::dedup::MergedEdge;
use crate::parse::{Cardinality, Schema};
use std::fmt::Write;

/// compose an attribute's display type, appending enum values when the type
/// names an enum defined in the schema.
///
/// enum-typed attributes render as `Type(A,B,C)` so every format surfaces the
/// allowed values in order. non-enum types — and enums with no values — return
/// the bare type name unchanged, so existing output and edge cases stay stable.
/// the result is unescaped; callers pass it through their own escaping helper.
pub fn attribute_type_display(schema: &Schema, type_name: &str) -> String {
    match schema.enum_values(type_name) {
        Some(values) if !values.is_empty() => format!("{}({})", type_name, values.join(",")),
        _ => type_name.to_string(),
    }
}

/// escape a string by replacing double quotes with single quotes.
///
/// used by formats (mermaid, plantuml) that quote strings with `"` and
/// have no backslash-escape mechanism.
pub fn escape_quotes(s: &str) -> String {
    s.replace('"', "'")
}

/// sanitize a string for use in unquoted attribute positions.
///
/// attribute lines in mermaid and plantuml are not wrapped in quotes, so
/// curly braces must be replaced to avoid breaking entity block syntax.
/// double quotes are replaced with single quotes as in [`escape_quotes`].
pub fn escape_attr(s: &str) -> String {
    escape_quotes(s).replace('{', "(").replace('}', ")")
}

/// render edges in crow's-foot notation, shared by mermaid and plantuml.
///
/// bidirectional edges are rendered with a combined label (`fwd / rev`),
/// unidirectional edges with a single label. entity names and field labels
/// are escaped with [`escape_quotes`].
pub fn render_edges(out: &mut String, edges: &[MergedEdge]) -> std::fmt::Result {
    for edge in edges {
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
                escape_quotes(&edge.left),
                left_card,
                right_card,
                escape_quotes(&edge.right),
                escape_quotes(&edge.left_to_right.field_name),
                escape_quotes(&rev.field_name)
            )?;
        } else {
            let cardinality = match edge.left_to_right.cardinality {
                Cardinality::One => "||--||",
                Cardinality::Many => "||--o{",
            };
            writeln!(
                out,
                "    \"{}\" {} \"{}\" : \"{}\"",
                escape_quotes(&edge.left),
                cardinality,
                escape_quotes(&edge.right),
                escape_quotes(&edge.left_to_right.field_name)
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub mod test_helpers {
    use crate::parse::{Attribute, Cardinality, Entity, EnumType, Relationship, Schema};

    /// build a standard test schema with InfraDevice and InfraInterface.
    ///
    /// shared across renderer test modules.
    pub fn test_schema() -> Schema {
        Schema {
            enums: vec![],
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

    /// build a test schema with one enum-typed attribute (`status: Status`) and
    /// one ordinary attribute (`hostname: TextAttribute`).
    ///
    /// shared across renderer test modules to assert enum-value rendering and
    /// that non-enum attributes stay unchanged.
    pub fn enum_schema() -> Schema {
        Schema {
            entities: vec![Entity {
                name: "Server".to_string(),
                attributes: vec![
                    Attribute {
                        name: "status".to_string(),
                        type_name: "Status".to_string(),
                    },
                    Attribute {
                        name: "hostname".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                ],
                relationships: vec![],
            }],
            enums: vec![EnumType {
                name: "Status".to_string(),
                values: vec!["ACTIVE".to_string(), "INACTIVE".to_string()],
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_quotes() {
        assert_eq!(escape_quotes("plain"), "plain");
        assert_eq!(escape_quotes(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_quotes(r#""""#), "''");
    }

    #[test]
    fn test_escape_attr() {
        assert_eq!(escape_attr("plain"), "plain");
        assert_eq!(escape_attr(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_attr("Set{String}"), "Set(String)");
        assert_eq!(escape_attr("no}close"), "no)close");
        assert_eq!(escape_attr("no{open"), "no(open");
    }

    #[test]
    fn test_attribute_type_display() {
        use crate::parse::EnumType;
        let schema = Schema {
            entities: vec![],
            enums: vec![
                EnumType {
                    name: "Status".to_string(),
                    values: vec!["ACTIVE".to_string(), "INACTIVE".to_string()],
                },
                EnumType {
                    name: "Empty".to_string(),
                    values: vec![],
                },
            ],
        };
        // enum type appends its values in declaration order
        assert_eq!(
            attribute_type_display(&schema, "Status"),
            "Status(ACTIVE,INACTIVE)"
        );
        // non-enum type is returned unchanged
        assert_eq!(
            attribute_type_display(&schema, "TextAttribute"),
            "TextAttribute"
        );
        // zero-value enum yields the bare type name — no stray parentheses
        assert_eq!(attribute_type_display(&schema, "Empty"), "Empty");
    }

    #[test]
    fn test_render_edges_bidirectional() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let edges = vec![MergedEdge {
            left: "A".to_string(),
            right: "B".to_string(),
            left_to_right: EdgeSide {
                field_name: "bs".to_string(),
                cardinality: Cardinality::Many,
            },
            right_to_left: Some(EdgeSide {
                field_name: "a".to_string(),
                cardinality: Cardinality::One,
            }),
        }];

        let mut out = String::new();
        render_edges(&mut out, &edges).unwrap();
        assert!(out.contains("\"A\" ||--o{ \"B\" : \"bs / a\""));
    }

    #[test]
    fn test_render_edges_unidirectional() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let edges = vec![MergedEdge {
            left: "A".to_string(),
            right: "B".to_string(),
            left_to_right: EdgeSide {
                field_name: "b".to_string(),
                cardinality: Cardinality::One,
            },
            right_to_left: None,
        }];

        let mut out = String::new();
        render_edges(&mut out, &edges).unwrap();
        assert!(out.contains("\"A\" ||--|| \"B\" : \"b\""));
    }

    #[test]
    fn test_render_edges_special_chars_in_labels() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let edges = vec![
            MergedEdge {
                left: "A".to_string(),
                right: "B".to_string(),
                left_to_right: EdgeSide {
                    field_name: "ref:link".to_string(),
                    cardinality: Cardinality::Many,
                },
                right_to_left: Some(EdgeSide {
                    field_name: "back|ref".to_string(),
                    cardinality: Cardinality::One,
                }),
            },
            MergedEdge {
                left: "C".to_string(),
                right: "D".to_string(),
                left_to_right: EdgeSide {
                    field_name: "items{0}".to_string(),
                    cardinality: Cardinality::One,
                },
                right_to_left: None,
            },
        ];

        let mut out = String::new();
        render_edges(&mut out, &edges).unwrap();
        // colons and pipes pass through inside quoted labels
        assert!(out.contains(r#""A" ||--o{ "B" : "ref:link / back|ref""#));
        // braces pass through inside quoted labels
        assert!(out.contains(r#""C" ||--|| "D" : "items{0}""#));
    }
}
