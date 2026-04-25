# changelog

## unreleased

- add `--include` and `--exclude` flags: regex patterns to filter entities by name, focusing diagrams on specific subsystems
- deduplicate bidirectional relationships: when two entities reference each other, render a single merged edge instead of two separate edges (DOT uses `headlabel`/`taillabel`; Mermaid uses a combined label)
- escape special characters (`"`, `\`, `{`, `}`, `|`, `<`, `>`) in DOT record labels to prevent malformed Graphviz output
- escape double quotes and quote entity names in Mermaid ER diagram output to handle schemas with spaces or special characters

## 0.2.0

- add mermaid er diagram output format (`--format mermaid`)
- fix list type cardinality: `[Type!]!` fields now correctly resolve to many instead of one
- bump `infrahub` dependency from 0.1.0 to 0.2.0

## 0.1.0

- initial release
- graphql schema parsing with entity, attribute, and relationship extraction
- graphviz dot output with record-shaped nodes and crow-foot cardinality
- live schema fetch via infrahub.rs client
- local schema file input (`--schema-file`)
- branch-aware schema fetch (`--branch`)
- attribute toggle (`--no-attributes`)
