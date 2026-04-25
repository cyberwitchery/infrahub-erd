//! entity name filtering
//!
//! applies `--include` and `--exclude` regex patterns to select which entities
//! appear in the output diagram. relationships pointing to excluded entities
//! are removed.

use crate::parse::Schema;
use regex::Regex;
use std::collections::HashSet;

/// filter a schema's entities by name using include/exclude regex patterns.
///
/// when `include` is set, only entities whose names match the pattern are kept.
/// when `exclude` is set, entities whose names match the pattern are removed.
/// when both are set, include is applied first, then exclude.
///
/// relationships targeting entities that were filtered out are removed.
pub fn filter_schema(schema: Schema, include: Option<&Regex>, exclude: Option<&Regex>) -> Schema {
    let kept: HashSet<String> = schema
        .entities
        .iter()
        .filter(|e| {
            if let Some(re) = include {
                if !re.is_match(&e.name) {
                    return false;
                }
            }
            if let Some(re) = exclude {
                if re.is_match(&e.name) {
                    return false;
                }
            }
            true
        })
        .map(|e| e.name.clone())
        .collect();

    let entities = schema
        .entities
        .into_iter()
        .filter(|e| kept.contains(&e.name))
        .map(|mut e| {
            e.relationships.retain(|r| kept.contains(&r.target));
            e
        })
        .collect();

    Schema { entities }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Attribute, Cardinality, Entity, Relationship};

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
                Entity {
                    name: "LocationSite".to_string(),
                    attributes: vec![Attribute {
                        name: "name".to_string(),
                        type_name: "TextAttribute".to_string(),
                    }],
                    relationships: vec![Relationship {
                        field_name: "devices".to_string(),
                        target: "InfraDevice".to_string(),
                        cardinality: Cardinality::Many,
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_no_filters() {
        let schema = filter_schema(test_schema(), None, None);
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["InfraDevice", "InfraInterface", "LocationSite"]);
    }

    #[test]
    fn test_include_only() {
        let re = Regex::new("^Infra").unwrap();
        let schema = filter_schema(test_schema(), Some(&re), None);
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["InfraDevice", "InfraInterface"]);
    }

    #[test]
    fn test_include_removes_dangling_relationships() {
        let re = Regex::new("^Infra").unwrap();
        let schema = filter_schema(test_schema(), Some(&re), None);
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        // site relationship to LocationSite should be gone
        assert_eq!(device.relationships.len(), 1);
        assert_eq!(device.relationships[0].target, "InfraInterface");
    }

    #[test]
    fn test_exclude_only() {
        let re = Regex::new("Interface").unwrap();
        let schema = filter_schema(test_schema(), None, Some(&re));
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["InfraDevice", "LocationSite"]);
    }

    #[test]
    fn test_exclude_removes_dangling_relationships() {
        let re = Regex::new("Interface").unwrap();
        let schema = filter_schema(test_schema(), None, Some(&re));
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        // interfaces relationship to InfraInterface should be gone
        assert_eq!(device.relationships.len(), 1);
        assert_eq!(device.relationships[0].target, "LocationSite");
    }

    #[test]
    fn test_include_and_exclude() {
        let include = Regex::new("^Infra").unwrap();
        let exclude = Regex::new("Interface$").unwrap();
        let schema = filter_schema(test_schema(), Some(&include), Some(&exclude));
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["InfraDevice"]);
        // all relationships point to excluded entities
        assert!(schema.entities[0].relationships.is_empty());
    }

    #[test]
    fn test_exclude_everything() {
        let re = Regex::new(".*").unwrap();
        let schema = filter_schema(test_schema(), None, Some(&re));
        assert!(schema.entities.is_empty());
    }

    #[test]
    fn test_include_matches_nothing() {
        let re = Regex::new("^NonExistent$").unwrap();
        let schema = filter_schema(test_schema(), Some(&re), None);
        assert!(schema.entities.is_empty());
    }

    #[test]
    fn test_attributes_preserved() {
        let re = Regex::new("InfraDevice").unwrap();
        let schema = filter_schema(test_schema(), Some(&re), None);
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        assert_eq!(device.attributes.len(), 1);
        assert_eq!(device.attributes[0].name, "name");
    }
}
