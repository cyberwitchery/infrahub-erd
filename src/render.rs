//! shared rendering utilities
//!
//! the [`Renderer`] trait plus [`render_document`] driver that every diagram
//! format is built on, along with the escaping and type-display helpers they
//! share.

use crate::dedup::{self, EdgeSide, MergedEdge};
use crate::error;
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

/// a diagram format expressed as a set of emit hooks.
///
/// [`render_document`] owns the shared control flow — the document preamble,
/// the entity walk, the single [`dedup::deduplicate`] pass over relationships,
/// and the trailer — and calls these hooks to emit each piece in the format's
/// own syntax. every hook writes directly into the output buffer and applies
/// the format's own escaping to `name`/`type_display` before emitting them.
pub trait Renderer {
    /// emit the document preamble (before any entity). defaults to nothing.
    fn document_header(&self, _out: &mut String) -> std::fmt::Result {
        Ok(())
    }

    /// emit the document trailer (after all edges). defaults to nothing.
    fn document_footer(&self, _out: &mut String) -> std::fmt::Result {
        Ok(())
    }

    /// open an entity block for `name`. `with_attributes` is true when the
    /// driver will follow with one or more [`attribute_line`](Self::attribute_line)
    /// calls, letting formats emit a compact form for attribute-less entities.
    fn entity_open(&self, out: &mut String, name: &str, with_attributes: bool) -> std::fmt::Result;

    /// emit one attribute row inside the open entity block.
    fn attribute_line(&self, out: &mut String, name: &str, type_display: &str) -> std::fmt::Result;

    /// close the entity block opened by [`entity_open`](Self::entity_open);
    /// `with_attributes` mirrors the value passed there.
    fn entity_close(&self, out: &mut String, with_attributes: bool) -> std::fmt::Result;

    /// emit a merged bidirectional edge, with `rev` as the right-to-left side.
    fn edge_bidirectional(
        &self,
        out: &mut String,
        edge: &MergedEdge,
        rev: &EdgeSide,
    ) -> std::fmt::Result;

    /// emit a unidirectional edge (`edge.left` → `edge.right`).
    fn edge_unidirectional(&self, out: &mut String, edge: &MergedEdge) -> std::fmt::Result;
}

/// render a schema into a diagram string using `renderer`'s format hooks.
///
/// walks the entities once and the deduplicated edges once, so adding a format
/// or changing the entity/edge skeleton is a change to this one driver rather
/// than to every renderer.
pub fn render_document(
    renderer: &impl Renderer,
    schema: &Schema,
    show_attributes: bool,
) -> error::Result<String> {
    let mut out = String::new();
    renderer.document_header(&mut out)?;

    for entity in &schema.entities {
        let with_attributes = show_attributes && !entity.attributes.is_empty();
        renderer.entity_open(&mut out, &entity.name, with_attributes)?;
        if with_attributes {
            for attr in &entity.attributes {
                let type_display = attribute_type_display(schema, &attr.type_name);
                renderer.attribute_line(&mut out, &attr.name, &type_display)?;
            }
        }
        renderer.entity_close(&mut out, with_attributes)?;
    }

    for edge in &dedup::deduplicate(schema) {
        match &edge.right_to_left {
            Some(rev) => renderer.edge_bidirectional(&mut out, edge, rev)?,
            None => renderer.edge_unidirectional(&mut out, edge)?,
        }
    }

    renderer.document_footer(&mut out)?;
    Ok(out)
}

/// emit a merged bidirectional edge in crow's-foot notation, shared by mermaid
/// and plantuml.
///
/// the combined label is `fwd / rev`; entity names and field labels are escaped
/// with [`escape_quotes`].
pub fn crowsfoot_edge_bidirectional(
    out: &mut String,
    edge: &MergedEdge,
    rev: &EdgeSide,
) -> std::fmt::Result {
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
    )
}

/// emit a unidirectional edge in crow's-foot notation, shared by mermaid and
/// plantuml.
///
/// entity names and the field label are escaped with [`escape_quotes`].
pub fn crowsfoot_edge_unidirectional(out: &mut String, edge: &MergedEdge) -> std::fmt::Result {
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
    )
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

    /// build a test schema chaining `InfraDevice.primary_group -> CoreGroup`
    /// and `CoreGroup.members -> CoreNode`.
    ///
    /// shared across renderer test modules to assert interface-derived entities
    /// render like any other.
    pub fn generic_schema() -> Schema {
        Schema {
            enums: vec![],
            entities: vec![
                Entity {
                    name: "CoreGroup".to_string(),
                    attributes: vec![Attribute {
                        name: "group_type".to_string(),
                        type_name: "TextAttribute".to_string(),
                    }],
                    relationships: vec![Relationship {
                        field_name: "members".to_string(),
                        target: "CoreNode".to_string(),
                        cardinality: Cardinality::Many,
                    }],
                },
                Entity {
                    name: "CoreNode".to_string(),
                    attributes: vec![],
                    relationships: vec![],
                },
                Entity {
                    name: "InfraDevice".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "primary_group".to_string(),
                        target: "CoreGroup".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        }
    }

    /// build a test schema with a single entity that references itself
    /// (`Tree.parent -> Tree`).
    ///
    /// shared across renderer test modules to assert self-loop edges render.
    pub fn self_ref_schema() -> Schema {
        Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "Tree".to_string(),
                attributes: vec![],
                relationships: vec![Relationship {
                    field_name: "parent".to_string(),
                    target: "Tree".to_string(),
                    cardinality: Cardinality::One,
                }],
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
    fn test_crowsfoot_edge_bidirectional() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let edge = MergedEdge {
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
        };
        let rev = edge.right_to_left.as_ref().unwrap();

        let mut out = String::new();
        crowsfoot_edge_bidirectional(&mut out, &edge, rev).unwrap();
        assert!(out.contains("\"A\" ||--o{ \"B\" : \"bs / a\""));
    }

    #[test]
    fn test_crowsfoot_edge_unidirectional() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let edge = MergedEdge {
            left: "A".to_string(),
            right: "B".to_string(),
            left_to_right: EdgeSide {
                field_name: "b".to_string(),
                cardinality: Cardinality::One,
            },
            right_to_left: None,
        };

        let mut out = String::new();
        crowsfoot_edge_unidirectional(&mut out, &edge).unwrap();
        assert!(out.contains("\"A\" ||--|| \"B\" : \"b\""));
    }

    #[test]
    fn test_crowsfoot_edge_special_chars_in_labels() {
        use crate::dedup::{EdgeSide, MergedEdge};
        use crate::parse::Cardinality;

        let bidi = MergedEdge {
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
        };
        let uni = MergedEdge {
            left: "C".to_string(),
            right: "D".to_string(),
            left_to_right: EdgeSide {
                field_name: "items{0}".to_string(),
                cardinality: Cardinality::One,
            },
            right_to_left: None,
        };

        let mut out = String::new();
        crowsfoot_edge_bidirectional(&mut out, &bidi, bidi.right_to_left.as_ref().unwrap())
            .unwrap();
        crowsfoot_edge_unidirectional(&mut out, &uni).unwrap();
        // colons and pipes pass through inside quoted labels
        assert!(out.contains(r#""A" ||--o{ "B" : "ref:link / back|ref""#));
        // braces pass through inside quoted labels
        assert!(out.contains(r#""C" ||--|| "D" : "items{0}""#));
    }
}
