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

    // types that implement AttributeInterface are attribute wrappers, not entities
    let attribute_types: HashSet<String> = object_types
        .iter()
        .filter(|(_, obj)| {
            obj.implements_interfaces
                .iter()
                .any(|i| i == "AttributeInterface")
        })
        .map(|(name, _)| name.clone())
        .collect();

    // check whether the schema uses infrahub node interfaces (CoreNode, CoreGroup).
    // when present, only types implementing these interfaces are entities.
    // when absent (e.g. partial or hand-crafted schemas), fall back to id-field heuristic.
    let has_node_interfaces = object_types
        .values()
        .any(|obj| implements_node_interface(obj));

    // identify entity types
    let entity_names: HashSet<String> = object_types
        .keys()
        .filter(|name| {
            is_entity_type(
                name,
                object_types.get(name.as_str()).unwrap(),
                &attribute_types,
                has_node_interfaces,
            )
        })
        .cloned()
        .collect();

    // extract entities with attributes and relationships
    let entities = entity_names
        .iter()
        .map(|name| {
            let obj = object_types[name];
            build_entity(name, obj, &entity_names, &attribute_types)
        })
        .collect::<Vec<_>>();

    // sort entities by name for stable output
    let mut entities = entities;
    entities.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Schema { entities })
}

/// determine if an object type represents a model entity
fn is_entity_type(
    name: &str,
    obj: &ObjectType<String>,
    attribute_types: &HashSet<String>,
    has_node_interfaces: bool,
) -> bool {
    // exclude root types
    if matches!(name, "Query" | "Mutation" | "Subscription" | "PageInfo") {
        return false;
    }

    // exclude types that implement AttributeInterface (Dropdown, IPHost, etc.)
    if attribute_types.contains(name) {
        return false;
    }

    // exclude attribute wrappers by name suffix
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

    // when the schema has CoreNode/CoreGroup interfaces, require them.
    // this filters out API infrastructure types (events, tasks, branches, etc.)
    if has_node_interfaces {
        return implements_node_interface(obj);
    }

    // fallback for schemas without interfaces: require an `id` field
    obj.fields.iter().any(|f| f.name == "id")
}

/// check if a type implements CoreNode or CoreGroup
fn implements_node_interface(obj: &ObjectType<String>) -> bool {
    obj.implements_interfaces
        .iter()
        .any(|i| i == "CoreNode" || i == "CoreGroup")
}

/// build an entity from an object type definition
fn build_entity(
    name: &str,
    obj: &ObjectType<String>,
    entity_names: &HashSet<String>,
    attribute_types: &HashSet<String>,
) -> Entity {
    let mut attributes = Vec::new();
    let mut relationships = Vec::new();

    for field in &obj.fields {
        // skip infrastructure fields
        if matches!(field.name.as_str(), "id" | "display_label" | "hfid") {
            continue;
        }

        let base_type = unwrap_type_name(&field.field_type);

        if let Some((target, cardinality)) = resolve_relationship(base_type, entity_names) {
            let cardinality = if is_list_type(&field.field_type) {
                Cardinality::Many
            } else {
                cardinality
            };
            relationships.push(Relationship {
                field_name: field.name.clone(),
                target,
                cardinality,
            });
        } else if is_attribute_type(base_type, attribute_types) {
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
fn is_attribute_type(name: &str, attribute_types: &HashSet<String>) -> bool {
    name.ends_with("Attribute") || attribute_types.contains(name)
}

/// unwrap a graphql type to its base named type
fn unwrap_type_name<'a>(ty: &'a GqlType<String>) -> &'a str {
    match ty {
        GqlType::NamedType(name) => name.as_str(),
        GqlType::NonNullType(inner) => unwrap_type_name(inner),
        GqlType::ListType(inner) => unwrap_type_name(inner),
    }
}

/// check whether a graphql type is wrapped in a list (possibly inside NonNull)
fn is_list_type(ty: &GqlType<String>) -> bool {
    match ty {
        GqlType::ListType(_) => true,
        GqlType::NonNullType(inner) => is_list_type(inner),
        GqlType::NamedType(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SCHEMA: &str = r#"
type Query {
  InfraDevice(name: String): InfraDevice
}

interface AttributeInterface {
  is_default: Boolean
}

type InfraDevice {
  id: String!
  display_label: String
  name: TextAttribute
  status: Dropdown
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

type TextAttribute implements AttributeInterface {
  id: String
  value: String
  is_default: Boolean
}

type NumberAttribute implements AttributeInterface {
  id: String
  value: Int
  is_default: Boolean
}

type Dropdown implements AttributeInterface {
  id: String
  value: String
  label: String
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
        assert_eq!(attr_names, ["name", "status"]);
        assert_eq!(device.attributes[0].type_name, "TextAttribute");
        assert_eq!(device.attributes[1].type_name, "Dropdown");
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
        assert!(!names.contains(&"NumberAttribute"));
        assert!(!names.contains(&"Dropdown"));
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

    /// covers: CoreNode interface detection, direct entity references,
    /// Attribute suffix exclusion without AttributeInterface, NonNull/List unwrap,
    /// Paginated/Edged/Related prefix resolution, and unknown field types
    #[test]
    fn test_core_node_schema() {
        let sdl = r#"
interface CoreNode { id: String! }
interface AttributeInterface { is_default: Boolean }

type Query { Node(name: String): NodeA }

type NodeA implements CoreNode {
  id: String!
  name: TextAttribute
  peer: NodeB
  items: PaginatedNodeB
  ref: EdgedNodeB
  link: RelatedNodeB
  tags: [NodeB!]!
  unknown_field: String
}

type NodeB implements CoreNode {
  id: String!
  label: TextAttribute
}

type TextAttribute implements AttributeInterface {
  value: String
}

type CustomAttribute {
  id: String
  value: String
}

type PaginatedNodeB { edges: [NodeB] }
type EdgedNodeB { node: NodeB }
type RelatedNodeB { node: NodeB }

type NotAnEntity { id: String! }
"#;
        let schema = parse_graphql_schema(sdl).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();

        // CoreNode entities only — NotAnEntity, CustomAttribute excluded
        assert!(names.contains(&"NodeA"));
        assert!(names.contains(&"NodeB"));
        assert!(!names.contains(&"NotAnEntity"));
        assert!(!names.contains(&"CustomAttribute"));

        let node_a = schema.entities.iter().find(|e| e.name == "NodeA").unwrap();

        // direct entity reference
        let peer = node_a
            .relationships
            .iter()
            .find(|r| r.field_name == "peer")
            .unwrap();
        assert_eq!(peer.target, "NodeB");
        assert_eq!(peer.cardinality, Cardinality::One);

        // Paginated prefix
        let items = node_a
            .relationships
            .iter()
            .find(|r| r.field_name == "items")
            .unwrap();
        assert_eq!(items.target, "NodeB");
        assert_eq!(items.cardinality, Cardinality::Many);

        // Edged prefix
        let edged = node_a
            .relationships
            .iter()
            .find(|r| r.field_name == "ref")
            .unwrap();
        assert_eq!(edged.target, "NodeB");
        assert_eq!(edged.cardinality, Cardinality::One);

        // Related prefix
        let related = node_a
            .relationships
            .iter()
            .find(|r| r.field_name == "link")
            .unwrap();
        assert_eq!(related.target, "NodeB");
        assert_eq!(related.cardinality, Cardinality::One);

        // [NodeB!]! is a list type wrapping an entity reference → Many
        let tags = node_a
            .relationships
            .iter()
            .find(|r| r.field_name == "tags")
            .unwrap();
        assert_eq!(tags.target, "NodeB");
        assert_eq!(tags.cardinality, Cardinality::Many);

        // unknown_field is String — neither attribute nor relationship, silently dropped
        assert!(node_a.attributes.iter().all(|a| a.name != "unknown_field"));
        assert!(node_a
            .relationships
            .iter()
            .all(|r| r.field_name != "unknown_field"));
    }
}
