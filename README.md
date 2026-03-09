# infrahub-erd

entity-relationship diagrams for infrahub.

## features

- fetches graphql schema from a live infrahub instance
- reads schema from a local `.graphql` file
- renders entity relationships as graphviz dot
- shows attributes and relationship cardinality (one vs. many)
- branch-aware schema fetch

## example

![demo topology](examples/demo.png)

generated from [examples/demo-schema.graphql](examples/demo-schema.graphql):

```bash
infrahub-erd -f examples/demo-schema.graphql --no-attributes | dot -Tpng -Gdpi=150 -o examples/demo.png
```

## docs

- [docs index](docs/index.md)
- [changelog](CHANGELOG.md)

## install

```bash
cargo install infrahub-erd
```

## quick start

from a live infrahub instance:

```bash
infrahub-erd --url http://localhost:8000 --token your-token > schema.dot
dot -Tpng schema.dot -o schema.png
```

from a schema file:

```bash
infrahub-erd --schema-file /path/to/schema.graphql > schema.dot
dot -Tsvg schema.dot -o schema.svg
```

hide attributes to get a cleaner relationship-only diagram:

```bash
infrahub-erd --schema-file schema.graphql --no-attributes > topo.dot
```

## environment variables

| variable | description |
|---|---|
| `INFRAHUB_URL` | infrahub instance url (alternative to `--url`) |
| `INFRAHUB_TOKEN` | api token (alternative to `--token`) |

## output

the default output is graphviz dot format. pipe it to `dot` for rendering:

```bash
# png
infrahub-erd -f schema.graphql | dot -Tpng -o schema.png

# svg
infrahub-erd -f schema.graphql | dot -Tsvg -o schema.svg

# pdf
infrahub-erd -f schema.graphql | dot -Tpdf -o schema.pdf
```

## development

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features
cargo fmt --all
```
