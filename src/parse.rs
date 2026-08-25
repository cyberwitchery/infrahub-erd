//! graphql schema parsing
//!
//! extracts entity types, attributes, and relationships from an infrahub
//! graphql schema sdl.

use crate::error::{Error, Result};
use graphql_parser::schema::{
    parse_schema, Definition, Field, InterfaceType, ObjectType, Type as GqlType, TypeDefinition,
};
use std::collections::{BTreeMap, HashSet};

/// parsed schema with entity types and their connections
#[derive(Debug)]
pub struct Schema {
    pub entities: Vec<Entity>,
    pub enums: Vec<EnumType>,
}

impl Schema {
    /// look up the allowed values of an enum type by name.
    ///
    /// returns `None` when the name does not refer to an enum, so callers can
    /// distinguish enum-typed attributes from ordinary ones.
    pub fn enum_values(&self, type_name: &str) -> Option<&[String]> {
        self.enums
            .iter()
            .find(|e| e.name == type_name)
            .map(|e| e.values.as_slice())
    }
}

/// a graphql enum type and its ordered list of allowed values
#[derive(Debug)]
pub struct EnumType {
    pub name: String,
    pub values: Vec<String>,
}

/// a model entity extracted from the graphql schema
#[derive(Debug)]
pub struct Entity {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub relationships: Vec<Relationship>,
    /// generics this entity implements, restricted to the ones drawn
    pub implements: Vec<String>,
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

    // collect enum types and their ordered values
    let mut enums: Vec<EnumType> = doc
        .definitions
        .iter()
        .filter_map(|def| match def {
            Definition::TypeDefinition(TypeDefinition::Enum(en)) => Some(EnumType {
                name: en.name.clone(),
                values: en.values.iter().map(|v| v.name.clone()).collect(),
            }),
            _ => None,
        })
        .collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    let enum_names: HashSet<String> = enums.iter().map(|e| e.name.clone()).collect();

    // collect interface types; infrahub emits its generics as graphql interfaces
    let interface_types: BTreeMap<String, &InterfaceType<String>> = doc
        .definitions
        .iter()
        .filter_map(|def| match def {
            Definition::TypeDefinition(TypeDefinition::Interface(iface)) => {
                Some((iface.name.clone(), iface))
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
    let concrete_names: HashSet<String> = object_types
        .iter()
        .filter(|(name, obj)| is_entity_type(name, obj, &attribute_types, has_node_interfaces))
        .map(|(name, _)| name.clone())
        .collect();

    let generic_names = referenced_generics(&object_types, &interface_types, &concrete_names);
    let entity_names: HashSet<String> = concrete_names.union(&generic_names).cloned().collect();

    // extract entities with attributes and relationships
    let entities = entity_names
        .iter()
        .map(|name| {
            let fields = object_types
                .get(name)
                .map(|obj| obj.fields.as_slice())
                .unwrap_or_else(|| interface_types[name].fields.as_slice());
            let implements = object_types
                .get(name)
                .map(|obj| obj.implements_interfaces.as_slice())
                .unwrap_or_else(|| interface_types[name].implements_interfaces.as_slice());
            build_entity(
                name,
                fields,
                &entity_names,
                &generic_names,
                &attribute_types,
                &enum_names,
                implemented_generics(implements, &generic_names),
            )
        })
        .collect::<Vec<_>>();

    // sort entities by name for stable output
    let mut entities = entities;
    entities.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Schema { entities, enums })
}

/// determine if an object type represents a model entity
fn is_entity_type(
    name: &str,
    obj: &ObjectType<String>,
    attribute_types: &HashSet<String>,
    has_node_interfaces: bool,
) -> bool {
    if is_excluded_name(name) {
        return false;
    }

    // exclude types that implement AttributeInterface (Dropdown, IPHost, etc.)
    if attribute_types.contains(name) {
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

/// exclude root types, attribute wrappers, and edge/connection wrappers by name
fn is_excluded_name(name: &str) -> bool {
    matches!(name, "Query" | "Mutation" | "Subscription" | "PageInfo")
        || name.ends_with("Attribute")
        || name.starts_with("NestedEdged")
        || name.starts_with("NestedPaginated")
        || name.starts_with("Edged")
        || name.starts_with("Paginated")
}

/// determine if an interface type may stand in for an infrahub generic.
/// `AttributeInterface` is excluded so its implementors stay attribute wrappers.
fn is_generic_candidate(name: &str) -> bool {
    name != "AttributeInterface" && !is_excluded_name(name)
}

/// select the generics that some entity's relationship field points at, to a
/// fixed point: a selected generic's own fields may target further generics.
fn referenced_generics<'a>(
    object_types: &BTreeMap<String, &'a ObjectType<'a, String>>,
    interface_types: &BTreeMap<String, &'a InterfaceType<'a, String>>,
    concrete_names: &HashSet<String>,
) -> HashSet<String> {
    let candidates: HashSet<String> = interface_types
        .keys()
        .filter(|name| is_generic_candidate(name))
        .cloned()
        .collect();
    let reachable: HashSet<String> = concrete_names.union(&candidates).cloned().collect();

    let mut selected: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = concrete_names.iter().cloned().collect();

    while !frontier.is_empty() {
        let mut next = Vec::new();
        for source in &frontier {
            let fields = object_types
                .get(source)
                .map(|obj| obj.fields.as_slice())
                .unwrap_or_else(|| interface_types[source].fields.as_slice());
            for field in fields {
                if is_infrastructure_field(&field.name) || is_universal_generic_field(&field.name) {
                    continue;
                }
                let Some((target, _)) =
                    resolve_relationship(unwrap_type_name(&field.field_type), &reachable)
                else {
                    continue;
                };
                if candidates.contains(&target) && selected.insert(target.clone()) {
                    next.push(target);
                }
            }
        }
        frontier = next;
    }

    selected
}

/// check if a type implements CoreNode or CoreGroup
fn implements_node_interface(obj: &ObjectType<String>) -> bool {
    obj.implements_interfaces
        .iter()
        .any(|i| i == "CoreNode" || i == "CoreGroup")
}

/// the interface every infrahub node implements, so an edge into it says nothing
fn is_universal_interface(name: &str) -> bool {
    name == "CoreNode"
}

/// select the generics a type implements that the diagram also draws
fn implemented_generics(implements: &[String], generic_names: &HashSet<String>) -> Vec<String> {
    let mut names: Vec<String> = implements
        .iter()
        .filter(|name| generic_names.contains(name.as_str()) && !is_universal_interface(name))
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    names
}

/// fields every infrahub node carries, which say nothing about the model
fn is_infrastructure_field(name: &str) -> bool {
    matches!(name, "id" | "display_label" | "hfid")
}

/// fields infrahub puts on every node to link it to a generic; drawing them
/// would collapse the diagram into a star around one hub.
fn is_universal_generic_field(name: &str) -> bool {
    matches!(
        name,
        "member_of_groups" | "subscriber_of_groups" | "profiles"
    )
}

/// build an entity from the fields of an object or interface type definition
fn build_entity(
    name: &str,
    fields: &[Field<'_, String>],
    entity_names: &HashSet<String>,
    generic_names: &HashSet<String>,
    attribute_types: &HashSet<String>,
    enum_names: &HashSet<String>,
    implements: Vec<String>,
) -> Entity {
    let mut attributes = Vec::new();
    let mut relationships = Vec::new();

    for field in fields {
        if is_infrastructure_field(&field.name) {
            continue;
        }

        let base_type = unwrap_type_name(&field.field_type);

        if let Some((target, cardinality)) = resolve_relationship(base_type, entity_names) {
            if generic_names.contains(&target) && is_universal_generic_field(&field.name) {
                continue;
            }
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
        } else if is_attribute_type(base_type, attribute_types) || enum_names.contains(base_type) {
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
        implements,
    }
}

/// prefixes that wrap an entity name to form a relationship type, checked in order
const RELATIONSHIP_PREFIXES: &[(&str, Cardinality)] = &[
    ("NestedPaginated", Cardinality::Many),
    ("NestedEdged", Cardinality::One),
    ("Paginated", Cardinality::Many),
    ("Edged", Cardinality::One),
    ("Related", Cardinality::One),
];

/// resolve a field type to a relationship target, if applicable
fn resolve_relationship(
    type_name: &str,
    entity_names: &HashSet<String>,
) -> Option<(String, Cardinality)> {
    if entity_names.contains(type_name) {
        return Some((type_name.to_string(), Cardinality::One));
    }

    for &(prefix, cardinality) in RELATIONSHIP_PREFIXES {
        if let Some(suffix) = type_name.strip_prefix(prefix) {
            if entity_names.contains(suffix) {
                return Some((suffix.to_string(), cardinality));
            }
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

    /// `CoreEndpoint` is reached from a concrete type, `CoreNamespace` only
    /// through `CoreEndpoint`, and `CoreNode` only through `CoreGroup`.
    const GENERIC_SCHEMA: &str = r#"
type Query {
  InfraDevice(name: String): InfraDevice
}

interface AttributeInterface {
  is_default: Boolean
}

interface CoreNode {
  id: String!
}

interface CoreGroup {
  id: String!
  name: TextAttribute
  members: NestedPaginatedCoreNode
}

interface CoreEndpoint {
  id: String!
  role: TextAttribute
  connector: NestedEdgedInfraDevice
  namespace: NestedEdgedCoreNamespace
}

interface CoreNamespace {
  id: String!
  label: TextAttribute
}

interface CoreUnreferenced {
  id: String!
  label: TextAttribute
}

type InfraDevice implements CoreNode {
  id: String!
  display_label: String
  name: TextAttribute
  endpoint: NestedEdgedCoreEndpoint
  primary_group: NestedEdgedCoreGroup
  member_of_groups: NestedPaginatedCoreGroup
  subscriber_of_groups: NestedPaginatedCoreGroup
  profiles: NestedPaginatedCoreNode
}

type InfraInterface implements CoreNode & CoreEndpoint & CoreUnreferenced {
  id: String!
  role: TextAttribute
}

type TextAttribute implements AttributeInterface {
  value: String
}

type NestedEdgedCoreEndpoint { node: CoreEndpoint }
type NestedEdgedCoreNamespace { node: CoreNamespace }
type NestedEdgedCoreGroup { node: CoreGroup }
type NestedPaginatedCoreGroup { edges: [EdgedCoreGroup] }
type EdgedCoreGroup { node: CoreGroup }
type NestedPaginatedCoreNode { edges: [EdgedCoreNode] }
type EdgedCoreNode { node: CoreNode }
type NestedEdgedInfraDevice { node: InfraDevice }
"#;

    #[test]
    fn test_referenced_generics_become_entities() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "CoreEndpoint",
                "CoreGroup",
                "CoreNamespace",
                "CoreNode",
                "InfraDevice",
                "InfraInterface"
            ]
        );
    }

    #[test]
    fn test_unreferenced_generics_and_attribute_interface_excluded() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"CoreUnreferenced"));
        assert!(!names.contains(&"AttributeInterface"));
        assert!(!names.contains(&"TextAttribute"));
    }

    #[test]
    fn test_relationship_to_generic_resolves() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();

        let endpoint = device
            .relationships
            .iter()
            .find(|r| r.field_name == "endpoint")
            .unwrap();
        assert_eq!(endpoint.target, "CoreEndpoint");
        assert_eq!(endpoint.cardinality, Cardinality::One);

        let group = device
            .relationships
            .iter()
            .find(|r| r.field_name == "primary_group")
            .unwrap();
        assert_eq!(group.target, "CoreGroup");
        assert_eq!(group.cardinality, Cardinality::One);
    }

    #[test]
    fn test_implemented_generics_are_read_from_implements_interfaces() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let iface = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraInterface")
            .unwrap();
        // CoreNode is a node interface and CoreUnreferenced is not drawn
        assert_eq!(iface.implements, ["CoreEndpoint"]);
    }

    #[test]
    fn test_core_node_is_not_an_implements_link() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        assert!(device.implements.is_empty());
    }

    #[test]
    fn test_core_group_is_an_implements_link() {
        let sdl = r#"
type Query { InfraDevice: InfraDevice }

interface CoreNode { id: String! }
interface CoreGroup { id: String! name: TextAttribute }

type InfraDevice implements CoreNode {
  id: String!
  primary_group: NestedEdgedCoreGroup
}

type CoreStandardGroup implements CoreNode & CoreGroup {
  id: String!
  name: TextAttribute
}

type TextAttribute implements AttributeInterface { value: String }
interface AttributeInterface { is_default: Boolean }
type NestedEdgedCoreGroup { node: CoreGroup }
"#;
        let schema = parse_graphql_schema(sdl).unwrap();
        let group = schema
            .entities
            .iter()
            .find(|e| e.name == "CoreStandardGroup")
            .unwrap();
        assert_eq!(group.implements, ["CoreGroup"]);
    }

    #[test]
    fn test_generic_implementing_a_generic_links_to_it() {
        let sdl = r#"
type Query { Node: InfraDevice }

interface CoreNode { id: String! }
interface CoreEndpoint { id: String! }
interface CoreInterfaceEndpoint implements CoreEndpoint { id: String! }

type InfraDevice implements CoreNode {
  id: String!
  endpoint: NestedEdgedCoreEndpoint
  port: NestedEdgedCoreInterfaceEndpoint
}

type NestedEdgedCoreEndpoint { node: CoreEndpoint }
type NestedEdgedCoreInterfaceEndpoint { node: CoreInterfaceEndpoint }
"#;
        let schema = parse_graphql_schema(sdl).unwrap();
        let endpoint = schema
            .entities
            .iter()
            .find(|e| e.name == "CoreInterfaceEndpoint")
            .unwrap();
        assert_eq!(endpoint.implements, ["CoreEndpoint"]);
    }

    #[test]
    fn test_attribute_interface_is_never_an_implements_link() {
        let schema = parse_graphql_schema(TEST_SCHEMA).unwrap();
        assert!(schema.entities.iter().all(|e| e.implements.is_empty()));
    }

    #[test]
    fn test_universal_generic_fields_suppressed() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let device = schema
            .entities
            .iter()
            .find(|e| e.name == "InfraDevice")
            .unwrap();
        let fields: Vec<&str> = device
            .relationships
            .iter()
            .map(|r| r.field_name.as_str())
            .collect();
        assert_eq!(fields, ["endpoint", "primary_group"]);
    }

    #[test]
    fn test_generic_carries_own_attributes_and_relationships() {
        let schema = parse_graphql_schema(GENERIC_SCHEMA).unwrap();
        let endpoint = schema
            .entities
            .iter()
            .find(|e| e.name == "CoreEndpoint")
            .unwrap();

        let attrs: Vec<&str> = endpoint
            .attributes
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(attrs, ["role"]);

        let connector = endpoint
            .relationships
            .iter()
            .find(|r| r.field_name == "connector")
            .unwrap();
        assert_eq!(connector.target, "InfraDevice");

        // a generic reached only through another generic still resolves
        let namespace = endpoint
            .relationships
            .iter()
            .find(|r| r.field_name == "namespace")
            .unwrap();
        assert_eq!(namespace.target, "CoreNamespace");

        let group = schema
            .entities
            .iter()
            .find(|e| e.name == "CoreGroup")
            .unwrap();
        let members = group
            .relationships
            .iter()
            .find(|r| r.field_name == "members")
            .unwrap();
        assert_eq!(members.target, "CoreNode");
        assert_eq!(members.cardinality, Cardinality::Many);
    }

    #[test]
    fn test_schema_without_interfaces_unaffected() {
        let sdl = r#"
type Query { Node: NodeA }

type NodeA {
  id: String!
  name: TextAttribute
  peer: NestedEdgedNodeB
  member_of_groups: NestedPaginatedNodeB
}

type NodeB {
  id: String!
}

type TextAttribute { value: String }
type NestedEdgedNodeB { node: NodeB }
type NestedPaginatedNodeB { edges: [NodeB] }
"#;
        let schema = parse_graphql_schema(sdl).unwrap();
        let names: Vec<&str> = schema.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["NodeA", "NodeB"]);

        // with no generics in play, member_of_groups keeps resolving as before
        let node = schema.entities.iter().find(|e| e.name == "NodeA").unwrap();
        let fields: Vec<&str> = node
            .relationships
            .iter()
            .map(|r| r.field_name.as_str())
            .collect();
        assert_eq!(fields, ["peer", "member_of_groups"]);
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

        // CoreNode entities only; NotAnEntity, CustomAttribute excluded
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

        // unknown_field is String: neither attribute nor relationship, silently dropped
        assert!(node_a.attributes.iter().all(|a| a.name != "unknown_field"));
        assert!(node_a
            .relationships
            .iter()
            .all(|r| r.field_name != "unknown_field"));
    }

    #[test]
    fn test_parse_enum() {
        let sdl = r#"
type Query { Node: NodeA }

interface AttributeInterface { is_default: Boolean }

enum Status { ACTIVE INACTIVE }

type NodeA {
  id: String!
  status: Status
  name: TextAttribute
}

type TextAttribute implements AttributeInterface {
  value: String
}
"#;
        let schema = parse_graphql_schema(sdl).unwrap();

        // the enum is captured with its values in declaration order
        assert_eq!(schema.enums.len(), 1);
        assert_eq!(schema.enums[0].name, "Status");
        let values: Vec<&str> = schema.enums[0].values.iter().map(|v| v.as_str()).collect();
        assert_eq!(values, ["ACTIVE", "INACTIVE"]);

        // an enum-typed field becomes an attribute referencing the enum type
        let node = schema.entities.iter().find(|e| e.name == "NodeA").unwrap();
        let status = node.attributes.iter().find(|a| a.name == "status").unwrap();
        assert_eq!(status.type_name, "Status");
        // ordinary attribute types are unaffected
        let name = node.attributes.iter().find(|a| a.name == "name").unwrap();
        assert_eq!(name.type_name, "TextAttribute");

        // enum_values resolves enum types and rejects everything else
        let looked: Vec<&str> = schema
            .enum_values("Status")
            .unwrap()
            .iter()
            .map(|v| v.as_str())
            .collect();
        assert_eq!(looked, ["ACTIVE", "INACTIVE"]);
        assert!(schema.enum_values("TextAttribute").is_none());
        assert!(schema.enum_values("Nonexistent").is_none());

        // an enum is never treated as an entity
        assert!(!schema.entities.iter().any(|e| e.name == "Status"));
    }

    #[test]
    fn test_parse_enum_unused() {
        let sdl = r#"
type Query { Node: NodeA }

enum Unused { A B C }

type NodeA {
  id: String!
}
"#;
        let schema = parse_graphql_schema(sdl).unwrap();

        // the enum is still captured even though no attribute references it
        assert_eq!(schema.enums.len(), 1);
        assert_eq!(schema.enums[0].name, "Unused");

        // an unused enum produces no phantom attributes on any entity
        let node = schema.entities.iter().find(|e| e.name == "NodeA").unwrap();
        assert!(node.attributes.is_empty());
    }

    #[test]
    fn test_parse_enum_wrapped_types() {
        // enum fields wrapped in non-null / list markers must still resolve to
        // the enum and be captured as attributes: unwrap_type_name strips the
        // NonNullType/ListType wrappers so `Status!` and `[Status!]` both reduce
        // to `Status`. real Infrahub schemas lean heavily on `Type!`, so this
        // pins the behavior against future refactors of unwrap_type_name.
        let sdl = r#"
type Query { Node: NodeA }

enum Status { ACTIVE INACTIVE }

type NodeA {
  id: String!
  nullable_status: Status
  required_status: Status!
  status_list: [Status!]!
}
"#;
        let schema = parse_graphql_schema(sdl).unwrap();
        let node = schema.entities.iter().find(|e| e.name == "NodeA").unwrap();

        // every wrapping shape resolves to the bare enum type name
        for field in ["nullable_status", "required_status", "status_list"] {
            let attr = node
                .attributes
                .iter()
                .find(|a| a.name == field)
                .unwrap_or_else(|| panic!("{field} should be captured as an attribute"));
            assert_eq!(attr.type_name, "Status");
        }

        // enum fields are attributes, never misrouted to relationships
        assert!(node.relationships.is_empty());
    }
}
