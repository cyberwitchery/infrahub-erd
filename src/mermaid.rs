//! mermaid output
//!
//! renders a parsed schema as a mermaid er diagram.

use crate::dedup;
use crate::error;
use crate::parse::Schema;
use crate::render::escape_quotes;
use std::fmt::Write;

/// render a schema as a mermaid er diagram string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    let mut out = String::new();
    writeln!(out, "erDiagram")?;

    for entity in &schema.entities {
        if show_attributes && !entity.attributes.is_empty() {
            let name = escape_quotes(&entity.name);
            writeln!(out, "    \"{}\" {{", name)?;
            for attr in &entity.attributes {
                writeln!(
                    out,
                    "        {} {}",
                    escape_quotes(&attr.type_name),
                    escape_quotes(&attr.name)
                )?;
            }
            writeln!(out, "    }}")?;
        }
    }

    let edges = dedup::deduplicate(schema);
    crate::render::render_edges(&mut out, &edges)?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};
    use crate::render::test_helpers::test_schema;

    #[test]
    fn test_render_with_attributes() {
        let mermaid = render(&test_schema(), true).unwrap();
        assert!(mermaid.starts_with("erDiagram\n"));
        assert!(mermaid.contains("\"InfraDevice\" {"));
        assert!(mermaid.contains("        TextAttribute name"));
        assert!(!mermaid.contains("\"InfraInterface\" {"));
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
        assert!(!mermaid.contains("\"InfraDevice\" {"));
        assert!(!mermaid.contains("TextAttribute"));
        assert!(
            mermaid.contains("\"InfraDevice\" ||--o{ \"InfraInterface\" : \"interfaces / device\"")
        );
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema { entities: vec![] };
        let mermaid = render(&schema, true).unwrap();
        assert_eq!(mermaid, "erDiagram\n");
    }

    #[test]
    fn test_escape_mermaid() {
        assert_eq!(escape_quotes("plain"), "plain");
        assert_eq!(escape_quotes(r#"with "quotes""#), "with 'quotes'");
        assert_eq!(escape_quotes(r#""""#), "''");
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
        let mermaid = render(&schema, true).unwrap();
        assert!(mermaid.contains("\"My Entity\" {"));
        assert!(mermaid.contains("\"My Entity\" ||--|| \"Other Entity\" : \"a 'rel'\""));
    }
}
