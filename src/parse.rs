//! graphql schema parsing
//!
//! extracts entity types, attributes, and relationships from an infrahub
//! graphql schema sdl.

use crate::error::{Error, Result};
use graphql_parser::schema::{
    parse_schema, Definition, ObjectType, Type as GqlType, TypeDefinition,
};
use std::collections::{BTreeMap, HashSet};

/// parsed schema with entity types and their connections
#[derive(Debug)]
pub struct Schema {
    pub entities: Vec<Entity>,
}

/// a model entity extracted from the graphql schema
#[derive(Debug)]
pub struct Entity {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub relationships: Vec<Relationship>,
}

/// a scalar attribute on an entity
#[derive(Debug)]
pub struct Attribute {
    pub name: String,
    pub type_name: String,
}

/// a relationship between two entities
#[derive(Debug)]
pub struct Relationship {
    pub field_name: String,
    pub target: String,
    pub cardinality: Cardinality,
}

/// relationship cardinality
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    One,
    Many,
}

/// parse a graphql sdl string into a schema topology
pub fn parse_graphql_schema(sdl: &str) -> Result<Schema> {
    let doc = parse_schema::<String>(sdl).map_err(|err| Error::Parse(err.to_string()))?;

    // collect all object types
    let object_types: BTreeMap<String, &ObjectType<String>> = doc
        .definitions
        .iter()
        .filter_map(|def| match def {
            Definition::TypeDefinition(TypeDefinition::Object(obj)) => {
                Some((obj.name.clone(), obj))
            }
            _ => None,
        })
        .collect();

    // identify entity types: object types with an `id` field, excluding
    // infrastructure types (attribute wrappers, edge/connection types, root types)
    let entity_names: HashSet<String> = object_types
        .keys()
        .filter(|name| is_entity_type(name, object_types.get(name.as_str()).unwrap()))
        .cloned()
        .collect();

    // extract entities with attributes and relationships
    let entities = entity_names
        .iter()
        .map(|name| {
            let obj = object_types[name];
            build_entity(name, obj, &entity_names)
        })
        .collect::<Vec<_>>();

    // sort entities by name for stable output
    let mut entities = entities;
    entities.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Schema { entities })
}

/// determine if an object type represents a model entity
fn is_entity_type(name: &str, obj: &ObjectType<String>) -> bool {
    // exclude root types
    if matches!(name, "Query" | "Mutation" | "Subscription" | "PageInfo") {
        return false;
    }

    // exclude attribute wrappers
    if name.ends_with("Attribute") {
        return false;
    }

    // exclude edge and connection wrappers
    if name.starts_with("NestedEdged")
        || name.starts_with("NestedPaginated")
        || name.starts_with("Edged")
        || name.starts_with("Paginated")
    {
        return false;
    }

    // must have an `id` field
    obj.fields.iter().any(|f| f.name == "id")
}

/// build an entity from an object type definition
fn build_entity(name: &str, obj: &ObjectType<String>, entity_names: &HashSet<String>) -> Entity {
    let mut attributes = Vec::new();
    let mut relationships = Vec::new();

    for field in &obj.fields {
        // skip infrastructure fields
        if matches!(field.name.as_str(), "id" | "display_label" | "hfid") {
            continue;
        }

        let base_type = unwrap_type_name(&field.field_type);

        if let Some((target, cardinality)) = resolve_relationship(base_type, entity_names) {
            relationships.push(Relationship {
                field_name: field.name.clone(),
                target,
                cardinality,
            });
        } else if is_attribute_type(base_type) {
            attributes.push(Attribute {
                name: field.name.clone(),
                type_name: base_type.to_string(),
            });
        }
    }

    Entity {
        name: name.to_string(),
        attributes,
        relationships,
    }
}

/// resolve a field type to a relationship target, if applicable
fn resolve_relationship(
    type_name: &str,
    entity_names: &HashSet<String>,
) -> Option<(String, Cardinality)> {
    // direct entity reference
    if entity_names.contains(type_name) {
        return Some((type_name.to_string(), Cardinality::One));
    }

    // NestedPaginated<X> -> many relationship
    if let Some(suffix) = type_name.strip_prefix("NestedPaginated") {
        if entity_names.contains(suffix) {
            return Some((suffix.to_string(), Cardinality::Many));
        }
    }

    // NestedEdged<X> -> one relationship
    if let Some(suffix) = type_name.strip_prefix("NestedEdged") {
        if entity_names.contains(suffix) {
            return Some((suffix.to_string(), Cardinality::One));
        }
    }

    // Paginated<X> -> many
    if let Some(suffix) = type_name.strip_prefix("Paginated") {
        if entity_names.contains(suffix) {
            return Some((suffix.to_string(), Cardinality::Many));
        }
    }

    // Edged<X> -> one
    if let Some(suffix) = type_name.strip_prefix("Edged") {
        if entity_names.contains(suffix) {
            return Some((suffix.to_string(), Cardinality::One));
        }
    }

    // RelatedX -> one
    if let Some(suffix) = type_name.strip_prefix("Related") {
        if entity_names.contains(suffix) {
            return Some((suffix.to_string(), Cardinality::One));
        }
    }

    None
}

/// check if a type name looks like an attribute wrapper
fn is_attribute_type(name: &str) -> bool {
    name.ends_with("Attribute")
}

/// unwrap a graphql type to its base named type
fn unwrap_type_name<'a>(ty: &'a GqlType<String>) -> &'a str {
    match ty {
        GqlType::NamedType(name) => name.as_str(),
        GqlType::NonNullType(inner) => unwrap_type_name(inner),
        GqlType::ListType(inner) => unwrap_type_name(inner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SCHEMA: &str = r#"
type Query {
  InfraDevice(name: String): InfraDevice
}

type InfraDevice {
  id: String!
  display_label: String
  name: TextAttribute
  interfaces: NestedPaginatedInfraInterface
  site: NestedEdgedLocationSite
}

type InfraInterface {
  id: String!
  display_label: String
  name: TextAttribute
  speed: NumberAttribute
  device: NestedEdgedInfraDevice
}

type LocationSite {
  id: String!
  display_label: String
  name: TextAttribute
  devices: NestedPaginatedInfraDevice
}

type TextAttribute {
  value: String
  is_default: Boolean
}

type NumberAttribute {
  value: Int
  is_default: Boolean
}

type NestedPaginatedInfraInterface {
  edges: [EdgedInfraInterface]
  count: Int
}

type NestedEdgedLocationSite {
  node: LocationSite
}

type NestedEdgedInfraDevice {
  node: InfraDevice
}

type NestedPaginatedInfraDevice {
  edges: [EdgedInfraDevice]
  count: Int
}

type EdgedInfraInterface {
  node: InfraInterface
}

type EdgedInfraDevice {
  node: InfraDevice
}

type PageInfo {
  hasNextPage: Boolean!
  endCursor: String
}
"#;

    #[test]
    fn test_parse_entities() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["InfraDevice", "InfraInterface", "LocationSite"]);
    }

    #[test]
    fn test_parse_attributes() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        let attr_names: Vec<&str> = device.attributes.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(attr_names, ["name"]);
        assert_eq!(device.attributes[0].type_name, "TextAttribute");
    }

    #[test]
    fn test_parse_relationships() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();

        assert_eq!(device.relationships.len(), 2);

        let ifaces = device
            .relationships
            .iter()
            .find(|r| r.field_name == "interfaces")
            .unwrap();
        assert_eq!(ifaces.target, "InfraInterface");
        assert_eq!(ifaces.cardinality, Cardinality::Many);

        let site = device
            .relationships
            .iter()
            .find(|r| r.field_name == "site")
            .unwrap();
        assert_eq!(site.target, "LocationSite");
        assert_eq!(site.cardinality, Cardinality::One);
    }

    #[test]
    fn test_parse_back_references() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let iface = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraInterface")
            .unwrap();

        let device_rel = iface
            .relationships
            .iter()
            .find(|r| r.field_name == "device")
            .unwrap();
        assert_eq!(device_rel.target, "InfraDevice");
        assert_eq!(device_rel.cardinality, Cardinality::One);
    }

    #[test]
    fn test_excludes_non_entities() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"Query"));
        assert!(!names.contains(&"TextAttribute"));
        assert!(!names.contains(&"NestedPaginatedInfraInterface"));
        assert!(!names.contains(&"EdgedInfraDevice"));
        assert!(!names.contains(&"PageInfo"));
    }

    #[test]
    fn test_interface_attributes() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        let iface = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraInterface")
            .unwrap();
        let attr_names: Vec<&str> = iface.attributes.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(attr_names, ["name", "speed"]);
    }

    #[test]
    fn test_parse_error() {
        let err = parse_graphql_schema("not valid graphql {{{");
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), Error::Parse(_)));
    }

    #[test]
    fn test_empty_schema() {
        let schema = parse_graphql_schema("type Query { ok: Boolean }").unwrap();
        assert!(schema.entities.is_empty());
    }
}
