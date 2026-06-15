//! json output
//!
//! serializes a parsed schema as a json document with entities, attributes,
//! relationships, and deduplicated edges for programmatic consumption.

use crate::dedup;
use crate::error;
use crate::parse::{Cardinality, Schema};
use serde::Serialize;

#[derive(Serialize)]
struct JsonSchema {
    entities: Vec<JsonEntity>,
    edges: Vec<JsonEdge>,
}

#[derive(Serialize)]
struct JsonEntity {
    name: String,
    attributes: Vec<JsonAttribute>,
    relationships: Vec<JsonRelationship>,
}

#[derive(Serialize)]
struct JsonAttribute {
    name: String,
    #[serde(rename = "type")]
    type_name: String,
}

#[derive(Serialize)]
struct JsonRelationship {
    field: String,
    target: String,
    cardinality: JsonCardinality,
}

#[derive(Serialize)]
struct JsonEdge {
    left: String,
    right: String,
    left_to_right: JsonEdgeSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    right_to_left: Option<JsonEdgeSide>,
}

#[derive(Serialize)]
struct JsonEdgeSide {
    field: String,
    cardinality: JsonCardinality,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum JsonCardinality {
    One,
    Many,
}

impl From<Cardinality> for JsonCardinality {
    fn from(c: Cardinality) -> Self {
        match c {
            Cardinality::One => JsonCardinality::One,
            Cardinality::Many => JsonCardinality::Many,
        }
    }
}

/// render a schema as a json string
pub fn render(schema: &Schema, show_attributes: bool) -> error::Result<String> {
    let edges = dedup::deduplicate(schema);

    let json_entities = schema
        .entities
        .iter()
        .map(|entity| {
            let attributes = if show_attributes {
                entity
                    .attributes
                    .iter()
                    .map(|a| JsonAttribute {
                        name: a.name.clone(),
                        type_name: a.type_name.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            };

            let relationships = entity
                .relationships
                .iter()
                .map(|r| JsonRelationship {
                    field: r.field_name.clone(),
                    target: r.target.clone(),
                    cardinality: r.cardinality.into(),
                })
                .collect();

            JsonEntity {
                name: entity.name.clone(),
                attributes,
                relationships,
            }
        })
        .collect();

    let json_edges = edges
        .iter()
        .map(|edge| JsonEdge {
            left: edge.left.clone(),
            right: edge.right.clone(),
            left_to_right: JsonEdgeSide {
                field: edge.left_to_right.field_name.clone(),
                cardinality: edge.left_to_right.cardinality.into(),
            },
            right_to_left: edge.right_to_left.as_ref().map(|rev| JsonEdgeSide {
                field: rev.field_name.clone(),
                cardinality: rev.cardinality.into(),
            }),
        })
        .collect();

    let doc = JsonSchema {
        entities: json_entities,
        edges: json_edges,
    };

    let mut out = serde_json::to_string_pretty(&doc)?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Entity, Relationship};
    use crate::render::test_helpers::test_schema;
    use serde_json::Value;

    #[test]
    fn test_render_with_attributes() {
        let json = render(&test_schema(), true).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        // entities present
        let entities = doc["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 2);

        // InfraDevice has attributes
        let device = &entities[0];
        assert_eq!(device["name"], "InfraDevice");
        let attrs = device["attributes"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0]["name"], "name");
        assert_eq!(attrs[0]["type"], "TextAttribute");

        // InfraDevice has relationships
        let rels = device["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0]["field"], "interfaces");
        assert_eq!(rels[0]["target"], "InfraInterface");
        assert_eq!(rels[0]["cardinality"], "many");
        assert_eq!(rels[1]["field"], "site");
        assert_eq!(rels[1]["target"], "LocationSite");
        assert_eq!(rels[1]["cardinality"], "one");

        // InfraInterface has no attributes
        let iface = &entities[1];
        assert_eq!(iface["name"], "InfraInterface");
        assert!(iface["attributes"].as_array().unwrap().is_empty());

        // edges present and deduplicated
        let edges = doc["edges"].as_array().unwrap();
        // InfraDevice<->InfraInterface (bidirectional) + InfraDevice->LocationSite (unidirectional)
        assert_eq!(edges.len(), 2);

        // bidirectional edge has right_to_left
        let bidi = edges
            .iter()
            .find(|e| e["right_to_left"].is_object())
            .unwrap();
        assert_eq!(bidi["left"], "InfraDevice");
        assert_eq!(bidi["right"], "InfraInterface");
        assert_eq!(bidi["left_to_right"]["field"], "interfaces");
        assert_eq!(bidi["left_to_right"]["cardinality"], "many");
        assert_eq!(bidi["right_to_left"]["field"], "device");
        assert_eq!(bidi["right_to_left"]["cardinality"], "one");

        // unidirectional edge omits right_to_left
        let uni = edges.iter().find(|e| e["right_to_left"].is_null()).unwrap();
        assert_eq!(uni["left"], "InfraDevice");
        assert_eq!(uni["right"], "LocationSite");
        assert_eq!(uni["left_to_right"]["field"], "site");
        assert_eq!(uni["left_to_right"]["cardinality"], "one");
    }

    #[test]
    fn test_render_without_attributes() {
        let json = render(&test_schema(), false).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        let device = &doc["entities"][0];
        assert_eq!(device["name"], "InfraDevice");
        assert!(device["attributes"].as_array().unwrap().is_empty());

        // edges still present
        assert!(!doc["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_render_empty_schema() {
        let schema = Schema { entities: vec![] };
        let json = render(&schema, true).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();
        assert!(doc["entities"].as_array().unwrap().is_empty());
        assert!(doc["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_output_is_valid_json() {
        let json = render(&test_schema(), true).unwrap();
        assert!(serde_json::from_str::<Value>(&json).is_ok());
    }

    #[test]
    fn test_output_ends_with_newline() {
        let json = render(&test_schema(), true).unwrap();
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn test_render_special_chars_in_names() {
        let schema = Schema {
            entities: vec![Entity {
                name: "My \"Entity\"".to_string(),
                attributes: vec![Attribute {
                    name: "field\nname".to_string(),
                    type_name: "Type<T>".to_string(),
                }],
                relationships: vec![Relationship {
                    field_name: "a\\rel".to_string(),
                    target: "Other|Node".to_string(),
                    cardinality: Cardinality::One,
                }],
            }],
        };

        let json = render(&schema, true).unwrap();
        let doc: Value = serde_json::from_str(&json).unwrap();

        // json handles escaping automatically
        assert_eq!(doc["entities"][0]["name"], "My \"Entity\"");
        assert_eq!(doc["entities"][0]["attributes"][0]["name"], "field\nname");
        assert_eq!(doc["entities"][0]["relationships"][0]["field"], "a\\rel");
    }
}
