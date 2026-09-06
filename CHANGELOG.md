# changelog

## 0.4.0

- fix mermaid diagrams failing to render when a schema has an enum-typed attribute: the inline value list was emitted as `Status(ACTIVE,INACTIVE)`, and mermaid's ER attribute lexer rejects a comma in that position up to and including mermaid 11.15, so the whole diagram errored out in the viewer. mermaid 11.16 (june 2026) started accepting the comma, but a pinned mermaid or an embedded renderer older than that still errors, so mermaid output now separates the values with `-`, which every version lexes. the DOT, PlantUML and D2 output is unchanged
- draw which concrete entities implement each generic: a generic used to appear as a floating node with relationships aimed at it, with nothing saying which nodes actually are that generic. implementors now get an inheritance edge to the generic in all four output formats, drawn distinctly from a relationship: a dashed hollow-headed arrow in DOT, a `--|>` generalization arrow in PlantUML, a dashed `is a` arrow in D2, and a non-identifying `is a` relationship in Mermaid, whose ER syntax has no inheritance arrow of its own. `CoreNode`, which every infrahub node implements, is not drawn, and `--include`/`--exclude` prune an inheritance edge along with the generic it points at

## 0.3.2

- fix D2 output silently dropping attributes named after a D2 reserved keyword: rows like `label`, `icon`, `height` or `constraint` were read by D2 as directives on the surrounding table rather than as columns, so the attribute vanished and the entity's own label, icon or size was overwritten by the attribute's type name. on a stock infrahub 1.10 schema this restores 39 attribute rows across 37 entities. attributes named after a style keyword (`fill`, `opacity`, …) made D2 reject the diagram outright, and now render too
- draw infrahub generics and the relationships that point at them: generics arrive as graphql interfaces and were previously invisible, so every relationship targeting one was silently dropped. on a stock infrahub 1.10 schema this restores 109 relationships across the 83 entities already drawn and adds the 19 generics they point at, each with its own attributes and relationships. the `member_of_groups`, `subscriber_of_groups` and `profiles` fields infrahub puts on every node are not drawn, since they resolve for every entity at once and would bury the diagram under a single hub node

## 0.3.1

- show enum types and their allowed values in ER diagrams: enum-typed attributes now render as `Status(ACTIVE,INACTIVE)` across all four output formats (DOT, Mermaid, PlantUML, D2), instead of hiding the choices behind a bare type name
- fix D2 renderer to escape edge labels and attribute type names through `format_key`, preventing malformed output when field names or types contain special characters
- fix PlantUML and Mermaid renderers to escape curly braces in attribute names and type names, preventing malformed entity blocks when types like `Set{String}` appear in a schema
- fix Mermaid renderer to replace spaces with underscores in attribute fields, preventing broken attribute lines for names with whitespace
- fix Mermaid renderer to emit entity declarations for entities with no attributes, matching the behaviour of the DOT, D2, and PlantUML renderers

## 0.3.0

- add D2 diagram output format (`--format d2`) with sql_table shapes for entities and crow's-foot edges for relationships
- add PlantUML ER diagram output format (`--format plant-uml`)
- add `--include` and `--exclude` flags: regex patterns to filter entities by name, with automatic pruning of relationships that point to filtered-out entities

## 0.2.1

- replace `unwrap()` calls in DOT and Mermaid renderers with `?` error propagation, so render failures surface as `Result::Err` instead of panicking
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
