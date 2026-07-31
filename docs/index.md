# docs index

entrypoints:
- [changelog](../CHANGELOG.md)
- [contributing](../CONTRIBUTING.md)

how to use this tool:

- fetch a schema from a live infrahub instance or read from a file
- produces an entity-relationship diagram in graphviz dot, mermaid, plantuml, or
  d2 format
- pipe dot output to `dot` for png, svg, or pdf rendering

quick start:

```bash
# from a live instance
INFRAHUB_URL=http://localhost:8000 INFRAHUB_TOKEN=... infrahub-erd > schema.dot
dot -Tpng schema.dot -o schema.png

# from a file
infrahub-erd --schema-file /path/to/schema.graphql > schema.dot
```

output formats:

`--format` selects the diagram syntax: `dot` (the default), `mermaid`,
`plant-uml`, or `d2`. render dot output with graphviz; the mermaid, plantuml,
and d2 output render with their respective tools.

filtering:

`--include` and `--exclude` take a regex matched against entity names. include
keeps only matching entities, exclude drops matching entities, and when both are
given include is applied first. relationships to filtered-out entities are
pruned so no dangling edges remain.

branch:

`--branch <name>` fetches the schema from a specific branch of a live instance;
it is ignored when reading from `--schema-file`.

entity detection:

infrahub-erd identifies model entities in the graphql schema by looking for
object types with an `id` field, excluding infrastructure types like attribute
wrappers (`TextAttribute`, `NumberAttribute`), edge/connection types
(`NestedEdged*`, `NestedPaginated*`), and root types (`Query`, `Mutation`).

relationships are detected from fields whose types reference other entities,
either directly or through infrahub's connection type naming conventions.

generics:

infrahub emits its generics (`CoreGroup`, `CoreGenericRepository`,
`BuiltinIPNamespace`, …) as graphql interfaces rather than object types. a
generic is drawn as an entity (with its own attributes and relationships)
when some entity has a relationship pointing at it, so relationships whose
target is a generic appear in the diagram instead of being dropped. a generic
nothing points at is left out, as is `AttributeInterface`, whose implementors
stay attribute wrappers.

infrahub gives every node a `member_of_groups`, `subscriber_of_groups` and
`profiles` field. these resolve for every entity at once, so they are not drawn;
including them would collapse the diagram into a star around `CoreGroup` and
`CoreProfile`.

attributes:

fields whose types are attribute wrappers (object types implementing
`AttributeInterface`, such as `TextAttribute` and `NumberAttribute`) render as
attributes on the entity. a field whose type is a graphql `enum` is shown as an
attribute too, with its allowed values listed inline: an
`enum DeviceStatus { ACTIVE MAINTENANCE DECOMMISSIONED }` used as
`status: DeviceStatus` renders as
`DeviceStatus(ACTIVE,MAINTENANCE,DECOMMISSIONED)`, so the choices stay visible in
the diagram instead of hidden behind the type name.
