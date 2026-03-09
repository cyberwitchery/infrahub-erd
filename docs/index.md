# docs index

entrypoints:
- [changelog](../CHANGELOG.md)
- [contributing](../CONTRIBUTING.md)

how to use this tool:

- fetch a schema from a live infrahub instance or read from a file
- produces graphviz dot output showing entity relationships
- pipe to `dot` for png, svg, or pdf rendering

quick start:

```bash
# from a live instance
INFRAHUB_URL=http://localhost:8000 INFRAHUB_TOKEN=... infrahub-erd > schema.dot
dot -Tpng schema.dot -o schema.png

# from a file
infrahub-erd --schema-file /path/to/schema.graphql > schema.dot
```

entity detection:

infrahub-erd identifies model entities in the graphql schema by looking for
object types with an `id` field, excluding infrastructure types like attribute
wrappers (`TextAttribute`, `NumberAttribute`), edge/connection types
(`NestedEdged*`, `NestedPaginated*`), and root types (`Query`, `Mutation`).

relationships are detected from fields whose types reference other entities,
either directly or through infrahub's connection type naming conventions.
