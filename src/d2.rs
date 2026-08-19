//! d2 output
//!
//! renders a parsed schema as a d2 diagram with sql_table shapes for entities
//! and crow's-foot edges for relationships.

use crate::dedup::{EdgeSide, MergedEdge};
use crate::error;
use crate::parse::{Cardinality, Schema};
use crate::render::{render_document, Renderer};
use std::fmt::Write;

/// d2 reserved keywords, which d2 only honours when they appear unquoted.
///
/// the union of the keyword tables in d2's `d2ast/keywords.go`, matched
/// case-insensitively as d2 does.
const RESERVED_KEYWORDS: &[&str] = &[
    "3d",
    "animated",
    "bold",
    "border-radius",
    "class",
    "classes",
    "constraint",
    "direction",
    "double-border",
    "fill",
    "fill-pattern",
    "filled",
    "font",
    "font-color",
    "font-size",
    "grid-columns",
    "grid-gap",
    "grid-rows",
    "height",
    "horizontal-gap",
    "icon",
    "italic",
    "label",
    "layers",
    "left",
    "link",
    "multiple",
    "near",
    "opacity",
    "scenarios",
    "shadow",
    "shape",
    "source-arrowhead",
    "steps",
    "stroke",
    "stroke-dash",
    "stroke-width",
    "style",
    "target-arrowhead",
    "text-transform",
    "tooltip",
    "top",
    "underline",
    "vars",
    "vertical-gap",
    "width",
];

/// whether a string can stand as a bare d2 identifier.
fn is_bare(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// wrap a string in double quotes, escaping internal quotes and backslashes.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// format a string as a d2 key, quoting special characters and reserved
/// keywords.
///
/// an unquoted reserved keyword is read by d2 as a directive on the enclosing
/// shape rather than as a key.
fn format_key(s: &str) -> String {
    if is_bare(s) && !RESERVED_KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(s)) {
        s.to_string()
    } else {
        quote(s)
    }
}

/// format a string as a d2 value, quoting if it contains special characters.
///
/// reserved keywords need no quoting here: d2 only recognises them in key
/// position.
fn format_value(s: &str) -> String {
    if is_bare(s) {
        s.to_string()
    } else {
        quote(s)
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
    render_document(&D2Renderer, schema, show_attributes)
}

/// d2 emitter: `sql_table` shapes for entities and crow's-foot arrowheads on
/// `--` edges.
struct D2Renderer;

impl Renderer for D2Renderer {
    fn entity_open(
        &self,
        out: &mut String,
        name: &str,
        _with_attributes: bool,
    ) -> std::fmt::Result {
        writeln!(out, "{}: {{", format_key(name))?;
        writeln!(out, "  shape: sql_table")
    }

    fn attribute_line(&self, out: &mut String, name: &str, type_display: &str) -> std::fmt::Result {
        writeln!(
            out,
            "  {}: {}",
            format_key(name),
            format_value(type_display)
        )
    }

    fn entity_close(&self, out: &mut String, _with_attributes: bool) -> std::fmt::Result {
        writeln!(out, "}}")
    }

    fn edge_bidirectional(
        &self,
        out: &mut String,
        edge: &MergedEdge,
        rev: &EdgeSide,
    ) -> std::fmt::Result {
        writeln!(
            out,
            "{} -- {}: {} / {} {{",
            format_key(&edge.left),
            format_key(&edge.right),
            format_value(&edge.left_to_right.field_name),
            format_value(&rev.field_name),
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
        writeln!(out, "}}")
    }

    fn edge_unidirectional(&self, out: &mut String, edge: &MergedEdge) -> std::fmt::Result {
        writeln!(
            out,
            "{} -- {}: {} {{",
            format_key(&edge.left),
            format_key(&edge.right),
            format_value(&edge.left_to_right.field_name),
        )?;
        writeln!(out, "  source-arrowhead.shape: cf-one")?;
        writeln!(
            out,
            "  target-arrowhead.shape: {}",
            arrowhead_shape(edge.left_to_right.cardinality),
        )?;
        writeln!(out, "}}")
    }

    fn inheritance_edge(&self, out: &mut String, child: &str, parent: &str) -> std::fmt::Result {
        writeln!(
            out,
            "{} -> {}: is a {{",
            format_key(child),
            format_key(parent)
        )?;
        writeln!(out, "  target-arrowhead.shape: triangle")?;
        writeln!(out, "  target-arrowhead.style.filled: false")?;
        writeln!(out, "  style.stroke-dash: 3")?;
        writeln!(out, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};
    use crate::render::test_helpers::{enum_schema, generic_schema, self_ref_schema, test_schema};

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
        // enum-typed attribute lists its allowed values in order (format_value
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
    fn test_format_key_quotes_reserved_keywords() {
        for keyword in RESERVED_KEYWORDS {
            assert_eq!(
                format_key(keyword),
                format!("\"{keyword}\""),
                "{keyword} must be quoted"
            );
        }
        // d2 matches keywords case-insensitively, so casing does not escape them
        assert_eq!(format_key("Label"), "\"Label\"");
        assert_eq!(format_key("HEIGHT"), "\"HEIGHT\"");
        // names that merely contain or extend a keyword stay bare
        assert_eq!(format_key("labels"), "labels");
        assert_eq!(format_key("shape_id"), "shape_id");
        assert_eq!(format_key("my-label"), "my-label");
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value("TextAttribute"), "TextAttribute");
        assert_eq!(format_value("Text/String"), "\"Text/String\"");
        assert_eq!(format_value(r#"with"quote"#), r#""with\"quote""#);
        assert_eq!(format_value(""), "\"\"");
        // reserved keywords carry no meaning in value position
        assert_eq!(format_value("label"), "label");
        assert_eq!(format_value("shape"), "shape");
    }

    #[test]
    fn test_render_reserved_keyword_attributes() {
        let schema = Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "CoreArtifact".to_string(),
                attributes: vec![
                    Attribute {
                        name: "label".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                    Attribute {
                        name: "icon".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                    Attribute {
                        name: "height".to_string(),
                        type_name: "NumberAttribute".to_string(),
                    },
                    Attribute {
                        name: "constraint".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                    Attribute {
                        name: "fill".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                    Attribute {
                        name: "name".to_string(),
                        type_name: "TextAttribute".to_string(),
                    },
                ],
                relationships: vec![],
                implements: vec![],
            }],
        };
        let d2 = render(&schema, true).unwrap();
        assert!(d2.contains("  \"label\": TextAttribute"));
        assert!(d2.contains("  \"icon\": TextAttribute"));
        assert!(d2.contains("  \"height\": NumberAttribute"));
        assert!(d2.contains("  \"constraint\": TextAttribute"));
        assert!(d2.contains("  \"fill\": TextAttribute"));
        assert!(d2.contains("  name: TextAttribute"));
        // the renderer's own shape directive is the only bare `shape:` line
        assert_eq!(d2.matches("\n  shape: sql_table\n").count(), 1);
    }

    #[test]
    fn test_render_reserved_keyword_entity_and_edge_names() {
        let schema = Schema {
            enums: vec![],
            entities: vec![Entity {
                name: "style".to_string(),
                attributes: vec![],
                relationships: vec![Relationship {
                    field_name: "link".to_string(),
                    target: "layers".to_string(),
                    cardinality: Cardinality::Many,
                }],
                implements: vec![],
            }],
        };
        let d2 = render(&schema, false).unwrap();
        assert!(d2.contains("\"style\": {"));
        // edge endpoints are keys and get quoted; the label is a value and does not
        assert!(d2.contains("\"style\" -- \"layers\": link {"));
    }

    #[test]
    fn test_renderer_keyword_lines_stay_unquoted() {
        let d2 = render(&test_schema(), true).unwrap();
        assert!(d2.contains("  shape: sql_table"));
        assert!(d2.contains("  source-arrowhead.shape: cf-one"));
        assert!(d2.contains("  target-arrowhead.shape: cf-many"));
        assert!(!d2.contains("\"shape\": sql_table"));
        assert!(!d2.contains("\"source-arrowhead\""));
        assert!(!d2.contains("\"target-arrowhead\""));
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
                implements: vec![],
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
                    implements: vec![],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "back\"ref".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                    implements: vec![],
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

    #[test]
    fn test_render_generic_entities() {
        let d2 = render(&generic_schema(), true).unwrap();
        // an interface-derived entity is a node like any other
        assert!(d2.contains("CoreGroup: {\n  shape: sql_table\n  group_type: TextAttribute\n}"));
        assert!(d2.contains("CoreNode: {\n  shape: sql_table\n}"));
        // concrete -> generic and generic -> generic both render
        assert!(d2.contains("InfraDevice -- CoreGroup: primary_group {"));
        assert!(d2.contains("CoreGroup -- CoreNode: members {"));
    }

    #[test]
    fn test_render_inheritance_edge() {
        let d2 = render(&generic_schema(), true).unwrap();
        assert!(d2.contains(
            "CoreRepository -> CoreGenericRepository: is a {\n  target-arrowhead.shape: triangle\n  target-arrowhead.style.filled: false\n  style.stroke-dash: 3\n}"
        ));
        // the relationship aimed at the same generic stays an undirected `--` edge
        assert!(d2.contains("InfraDevice -- CoreGenericRepository: repository {"));
    }
}
