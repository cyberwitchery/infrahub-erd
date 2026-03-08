# contributing

short, practical notes for working on this repo.

## local dev commands

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features
cargo fmt --all
```

## docs build

```bash
RUSTDOCFLAGS="--cfg docsrs" cargo doc --all-features --no-deps
```

open `target/doc/infrahub_topo/index.html` for the docs.

## coverage

we use `cargo llvm-cov`.

install:

```bash
cargo install cargo-llvm-cov
```

run:

```bash
cargo llvm-cov --all-features
```

generate lcov (for ci or tooling):

```bash
cargo llvm-cov --all-features --lcov --output-path lcov.info
```

ci enforces a minimum line coverage of 80%.

## documentation

- add rustdoc for public apis
- include examples for new features
- update `README.md` and `CHANGELOG.md` for user-visible changes

## release checklist (maintainers)

1. update `CHANGELOG.md`
2. bump version in `Cargo.toml`
3. run tests and coverage
4. tag commit with `v0.X.Y`
5. push tag — github actions handles the rest

## release automation

we publish from tags matching `v*` (for example, `v0.1.0`).

the release workflow expects a repository secret named `CARGO_REGISTRY_TOKEN`.

## issue reporting

use https://github.com/cyberwitchery/infrahub-topo/issues
