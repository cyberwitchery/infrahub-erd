//! d2 output
//!
//! renders a parsed schema as a d2 diagram with sql_table shapes for entities
//! and crow's-foot edges for relationships.

use crate::dedup::{self, MergedEdge};
use crate::error;
use crate::parse::{Cardinality, Schema};
use crate::render::attribute_type_display;
use std::fmt::Write;

/// format a string as a d2 key, quoting if it contains special characters.
///
/// d2 keys containing only alphanumeric characters, underscores, and hyphens
/// are used verbatim. keys with other characters (spaces, punctuation, d2
/// syntax characters) are wrapped in double quotes with internal quotes and
/// backslashes escaped.
fn format_key(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// map a relationship cardinality to a d2 crow's-foot arrowhead shape.
fn arrowhead_shape(c: Cardinality) -> &'static str {
    match c {
        Cardinality::One => "cf-one",
        Cardinality::Many => "cf-many",
    }
}

/// render a schema as a d2 diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    let mut out = String::new();

    for entity in &schema.entities {
        let key = format_key(&entity.name);
        writeln!(out, "{}: {{", key)?;
        writeln!(out, "  shape: sql_table")?;
        if show_attributes {
            for attr in &entity.attributes {
                writeln!(
                    out,
                    "  {}: {}",
                    format_key(&attr.name),
                    format_key(&attribute_type_display(schema, &attr.type_name))
                )?;
            }
        }
        writeln!(out, "}}")?;
    }

    let edges = dedup::deduplicate(schema);
    render_edges(&mut out, &edges)?;

    Ok(out)
}

/// render relationship edges with crow's-foot arrowheads.
///
/// bidirectional edges use `--` with a combined label (`fwd / rev`) and
/// arrowheads on both sides. unidirectional edges use `--` with cf-one on
/// the source side and the appropriate cardinality on the target side.
fn render_edges(out: &mut String, edges: &[MergedEdge]) -> std::fmt::Result {
    for edge in edges {
        let left = format_key(&edge.left);
        let right = format_key(&edge.right);

        if let Some(ref rev) = edge.right_to_left {
            writeln!(
                out,
                "{} -- {}: {} / {} {{",
                left,
                right,
                format_key(&edge.left_to_right.field_name),
                format_key(&rev.field_name),
            )?;
            writeln!(
                out,
                "  source-arrowhead.shape: {}",
                arrowhead_shape(rev.cardinality),
            )?;
            writeln!(
                out,
                "  target-arrowhead.shape: {}",
                arrowhead_shape(edge.left_to_right.cardinality),
            )?;
            writeln!(out, "}}")?;
        } else {
            writeln!(
                out,
                "{} -- {}: {} {{",
                left,
                right,
                format_key(&edge.left_to_right.field_name),
            )?;
            writeln!(out, "  source-arrowhead.shape: cf-one")?;
            writeln!(
                out,
                "  target-arrowhead.shape: {}",
                arrowhead_shape(edge.left_to_right.cardinality),
            )?;
            writeln!(out, "}}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};
    use crate::render::test_helpers::{enum_schema, self_ref_schema, test_schema};

    #[test]
    fn test_render_with_attributes() {
        let d2 = render(&test_schema(), true).unwrap();
        assert!(d2.contains("InfraDevice: {"));
        assert!(d2.contains("  shape: sql_table"));
        assert!(d2.contains("  name: TextAttribute"));
        // InfraInterface has no attributes, just the shape
        assert!(d2.contains("InfraInterface: {\n  shape: sql_table\n}"));
        // bidirectional edge with crow's-foot arrowheads
        assert!(d2.contains("InfraDevice -- InfraInterface: interfaces / device {"));
        assert!(d2.contains("  source-arrowhead.shape: cf-one"));
        assert!(d2.contains("  target-arrowhead.shape: cf-many"));
        // unidirectional edge
        assert!(d2.contains("InfraDevice -- LocationSite: site {"));
        // reverse edge not rendered separately
        assert!(!d2.contains("InfraInterface -- InfraDevice"));
    }

    #[test]
    fn test_render_without_attributes() {
        let d2 = render(&test_schema(), false).unwrap();
        assert!(d2.contains("InfraDevice: {\n  shape: sql_table\n}"));
        assert!(!d2.contains("TextAttribute"));
        // edges still present
        assert!(d2.contains("InfraDevice -- InfraInterface: interfaces / device {"));
    }

    #[test]
    fn test_render_enum_attribute() {
        let d2 = render(&enum_schema(), true).unwrap();
        // enum-typed attribute lists its allowed values in order (format_key
        // quotes the value because it contains parentheses)
        assert!(d2.contains("status: \"Status(ACTIVE,INACTIVE)\""));
        // non-enum attribute is unchanged: bare unquoted type, no value list
        assert!(d2.contains("hostname: TextAttribute"));
        assert!(!d2.contains("TextAttribute("));
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema {
            entities: vec![],
            enums: vec![],
        };
        let d2 = render(&schema, true).unwrap();
        assert_eq!(d2, "");
    }

    #[test]
    fn test_format_key() {
        assert_eq!(format_key("plain"), "plain");
        assert_eq!(format_key("CamelCase"), "CamelCase");
        assert_eq!(format_key("with-hyphen"), "with-hyphen");
        assert_eq!(format_key("under_score"), "under_score");
        assert_eq!(format_key("with space"), "\"with space\"");
        assert_eq!(format_key("with|pipe"), "\"with|pipe\"");
        assert_eq!(format_key(r#"with"quote"#), r#""with\"quote""#);
        assert_eq!(format_key(r"back\slash"), r#""back\\slash""#);
        assert_eq!(format_key(""), "\"\"");
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
                    field_name: "rel".to_string(),
                    target: "Other Entity".to_string(),
                    cardinality: Cardinality::One,
                }],
            }],
        };
        let d2 = render(&schema, true).unwrap();
        assert!(d2.contains("\"My Entity\": {"));
        assert!(d2.contains("  \"full name\": Text"));
        assert!(d2.contains("\"My Entity\" -- \"Other Entity\": rel {"));
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
                        type_name: "Text/String".to_string(),
                    }],
                    relationships: vec![Relationship {
                        field_name: "my ref".to_string(),
                        target: "B".to_string(),
                        cardinality: Cardinality::Many,
                    }],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "back\"ref".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        };
        let d2 = render(&schema, true).unwrap();
        // type_name with slash is quoted
        assert!(d2.contains(r#"  path: "Text/String""#));
        // bidirectional edge with special chars in both labels
        assert!(d2.contains(r#"A -- B: "my ref" / "back\"ref" {"#));
    }

    #[test]
    fn test_render_self_referential_edge() {
        let d2 = render(&self_ref_schema(), false).unwrap();
        // self-loop edge: left == right
        assert!(d2.contains("Tree -- Tree: parent {"));
    }
}
