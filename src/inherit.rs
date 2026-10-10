//! inherited member pruning
//!
//! drops the attributes and relationships an entity inherits unchanged from a
//! generic it implements, so each one is drawn once, on the generic.

use crate::parse::{Entity, Schema};
use std::collections::HashMap;

/// copy `schema` without the members each entity inherits unchanged from a
/// generic in its `implements` list.
///
/// an attribute is inherited when the generic declares one with the same name
/// and type, a relationship when it declares one with the same field name,
/// target and cardinality. a member the entity narrows or changes stays.
pub fn without_inherited_members(schema: &Schema) -> Schema {
    let by_name: HashMap<&str, &Entity> = schema
        .entities
        .iter()
        .map(|e| (e.name.as_str(), e))
        .collect();

    let entities = schema
        .entities
        .iter()
        .map(|entity| {
            let generics: Vec<&Entity> = entity
                .implements
                .iter()
                .filter_map(|name| by_name.get(name.as_str()).copied())
                .collect();
            Entity {
                name: entity.name.clone(),
                attributes: entity
                    .attributes
                    .iter()
                    .filter(|attr| !generics.iter().any(|g| g.attributes.contains(attr)))
                    .cloned()
                    .collect(),
                relationships: entity
                    .relationships
                    .iter()
                    .filter(|rel| !generics.iter().any(|g| g.relationships.contains(rel)))
                    .cloned()
                    .collect(),
                implements: entity.implements.clone(),
            }
        })
        .collect();

    Schema {
        entities,
        enums: schema.enums.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::filter_schema;
    use crate::parse::{Attribute, Cardinality, Relationship};
    use crate::render::test_helpers::inherited_schema;
    use regex::Regex;

    fn attribute(name: &str, type_name: &str) -> Attribute {
        Attribute {
            name: name.to_string(),
            type_name: type_name.to_string(),
        }
    }

    fn relationship(field_name: &str, target: &str, cardinality: Cardinality) -> Relationship {
        Relationship {
            field_name: field_name.to_string(),
            target: target.to_string(),
            cardinality,
        }
    }

    fn node(
        name: &str,
        attributes: Vec<Attribute>,
        relationships: Vec<Relationship>,
        implements: &[&str],
    ) -> Entity {
        Entity {
            name: name.to_string(),
            attributes,
            relationships,
            implements: implements.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn entity<'a>(schema: &'a Schema, name: &str) -> &'a Entity {
        schema.entities.iter().find(|e| e.name == name).unwrap()
    }

    fn attribute_names(entity: &Entity) -> Vec<&str> {
        entity.attributes.iter().map(|a| a.name.as_str()).collect()
    }

    fn field_names(entity: &Entity) -> Vec<&str> {
        entity
            .relationships
            .iter()
            .map(|r| r.field_name.as_str())
            .collect()
    }

    #[test]
    fn test_inherited_attributes_are_dropped() {
        let schema = without_inherited_members(&inherited_schema());
        assert_eq!(attribute_names(entity(&schema, "CoreGroup")), ["name"]);
        assert_eq!(
            attribute_names(entity(&schema, "CoreRepositoryGroup")),
            ["content"]
        );
        assert!(entity(&schema, "CoreStandardGroup").attributes.is_empty());
    }

    #[test]
    fn test_inherited_relationships_are_dropped() {
        let schema = without_inherited_members(&inherited_schema());
        assert_eq!(field_names(entity(&schema, "CoreGroup")), ["members"]);
        assert!(entity(&schema, "CoreRepositoryGroup")
            .relationships
            .is_empty());
        assert!(entity(&schema, "CoreStandardGroup")
            .relationships
            .is_empty());
        assert_eq!(
            field_names(entity(&schema, "InfraDevice")),
            ["standard_group"]
        );
    }

    #[test]
    fn test_changed_or_uninherited_members_are_kept() {
        let schema = Schema {
            enums: vec![],
            entities: vec![
                node(
                    "Generic",
                    vec![attribute("status", "TextAttribute")],
                    vec![relationship("peer", "Generic", Cardinality::One)],
                    &[],
                ),
                node(
                    "Retyped",
                    vec![attribute("status", "Dropdown")],
                    vec![relationship("peer", "Retyped", Cardinality::One)],
                    &["Generic"],
                ),
                node(
                    "Widened",
                    vec![],
                    vec![relationship("peer", "Generic", Cardinality::Many)],
                    &["Generic"],
                ),
                node(
                    "Unrelated",
                    vec![attribute("status", "TextAttribute")],
                    vec![relationship("peer", "Generic", Cardinality::One)],
                    &[],
                ),
                node(
                    "Copied",
                    vec![attribute("status", "TextAttribute")],
                    vec![relationship("peer", "Generic", Cardinality::One)],
                    &["Generic"],
                ),
            ],
        };
        let pruned = without_inherited_members(&schema);
        let copied = entity(&pruned, "Copied");
        assert!(copied.attributes.is_empty() && copied.relationships.is_empty());
        for name in ["Generic", "Retyped", "Widened", "Unrelated"] {
            let (before, after) = (entity(&schema, name), entity(&pruned, name));
            assert_eq!(after.attributes, before.attributes, "{name}");
            assert_eq!(after.relationships, before.relationships, "{name}");
        }
    }

    #[test]
    fn test_generic_dropped_by_filter_keeps_implementor_members() {
        let exclude = Regex::new("^CoreGroup$").unwrap();
        let filtered = filter_schema(inherited_schema(), None, Some(&exclude));
        let schema = without_inherited_members(&filtered);
        let group = entity(&schema, "CoreStandardGroup");
        assert_eq!(attribute_names(group), ["name"]);
        assert_eq!(field_names(group), ["members"]);
    }

    #[test]
    fn test_generic_chain_draws_each_member_where_it_is_first_declared() {
        let connected = || relationship("connected", "CoreEndpoint", Cardinality::One);
        let schema = Schema {
            enums: vec![],
            entities: vec![
                node(
                    "CoreEndpoint",
                    vec![attribute("role", "TextAttribute")],
                    vec![connected()],
                    &[],
                ),
                node(
                    "CoreInterfaceEndpoint",
                    vec![
                        attribute("role", "TextAttribute"),
                        attribute("speed", "NumberAttribute"),
                    ],
                    vec![connected()],
                    &["CoreEndpoint"],
                ),
                node(
                    "InfraInterface",
                    vec![
                        attribute("role", "TextAttribute"),
                        attribute("speed", "NumberAttribute"),
                        attribute("mtu", "NumberAttribute"),
                    ],
                    vec![connected()],
                    &["CoreInterfaceEndpoint"],
                ),
            ],
        };
        let pruned = without_inherited_members(&schema);
        assert_eq!(attribute_names(entity(&pruned, "CoreEndpoint")), ["role"]);
        assert_eq!(
            attribute_names(entity(&pruned, "CoreInterfaceEndpoint")),
            ["speed"]
        );
        assert_eq!(attribute_names(entity(&pruned, "InfraInterface")), ["mtu"]);
        assert_eq!(field_names(entity(&pruned, "CoreEndpoint")), ["connected"]);
        assert!(entity(&pruned, "CoreInterfaceEndpoint")
            .relationships
            .is_empty());
        assert!(entity(&pruned, "InfraInterface").relationships.is_empty());
    }
}
