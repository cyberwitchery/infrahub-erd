//! end-to-end tests for the `infrahub-erd` binary.
//!
//! these drive the compiled cli against the committed
//! `examples/demo-schema.graphql` fixture, so they exercise the whole
//! parse -> filter -> render -> output pipeline that the per-renderer unit
//! tests cannot reach on their own. every `--format` value is checked for its
//! signature output, and the core flags (`--no-attributes`, `--include`,
//! `--exclude`, `--output`) and error paths are covered here rather than by
//! hand.

use std::path::PathBuf;
use std::process::{Command, Output};

/// every entity declared in the demo schema's `Query` root. all four formats
/// are expected to surface every one of them.
const DEMO_ENTITIES: &[&str] = &[
    "InfraDevice",
    "InfraInterface",
    "InfraIPAddress",
    "InfraIPPrefix",
    "InfraVLAN",
    "LocationSite",
    "LocationRack",
    "OrganizationTenant",
    "BuiltinTag",
];

/// the four accepted `--format` values, in `--help` order.
const FORMATS: &[&str] = &["dot", "mermaid", "plant-uml", "d2"];

fn demo_schema() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo-schema.graphql")
}

/// run the binary with `args` and return the raw process output.
///
/// `INFRAHUB_URL`/`INFRAHUB_TOKEN` are cleared so the suite is hermetic: the
/// `--url`/`--token` clap args default to those vars, so a developer with them
/// set in their shell would otherwise change the no-source error path (or make
/// a real network fetch) under `missing_schema_source_is_an_error`.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_infrahub-erd"))
        .env_remove("INFRAHUB_URL")
        .env_remove("INFRAHUB_TOKEN")
        .args(args)
        .output()
        .expect("failed to spawn infrahub-erd")
}

/// run the binary against the demo schema, assert it succeeds, and return its
/// stdout as a string. `extra` is appended after `--schema-file <demo>`.
fn render(extra: &[&str]) -> String {
    let schema = demo_schema();
    let mut args = vec!["--schema-file", schema.to_str().unwrap()];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(
        out.status.success(),
        "expected success for args {extra:?}, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout was not utf-8")
}

#[test]
fn default_format_is_dot() {
    // running without --format must be byte-for-byte identical to --format dot.
    assert_eq!(render(&[]), render(&["--format", "dot"]));
}

#[test]
fn dot_signature() {
    let out = render(&["--format", "dot"]);
    assert!(out.starts_with("digraph schema {"));
    assert!(out.contains("rankdir=LR;"));
    assert!(out.contains("node [shape=record"));
    // a known relationship, rendered as a dot edge.
    assert!(out.contains("\"InfraDevice\" -> \"InfraInterface\""));
}

#[test]
fn bidirectional_edge_is_merged_in_dot() {
    // InfraDevice.interfaces (many) <-> InfraInterface.device (one) is a
    // bidirectional pair: the two directions collapse into a single dir=both
    // edge that carries both field names as tail/head labels.
    let out = render(&["--format", "dot"]);
    assert!(out.contains(
        "\"InfraDevice\" -> \"InfraInterface\" [taillabel=\"interfaces\", headlabel=\"device\", arrowhead=crow, arrowtail=normal, dir=both];"
    ));
    // the reverse direction is folded in, never rendered as its own edge.
    assert!(!out.contains("\"InfraInterface\" -> \"InfraDevice\""));
}

#[test]
fn mermaid_signature() {
    let out = render(&["--format", "mermaid"]);
    assert!(out.starts_with("erDiagram"));
    assert!(out.contains("\"InfraDevice\" {"));
    // crow's-foot edge between a device and its interfaces.
    assert!(out.contains("\"InfraDevice\" ||--o{ \"InfraInterface\""));
}

#[test]
fn plantuml_signature() {
    let out = render(&["--format", "plant-uml"]);
    assert!(out.starts_with("@startuml"));
    assert!(out.trim_end().ends_with("@enduml"));
    assert!(out.contains("entity \"InfraDevice\" {"));
}

#[test]
fn d2_signature() {
    let out = render(&["--format", "d2"]);
    assert!(out.contains("InfraDevice: {"));
    assert!(out.contains("shape: sql_table"));
}

#[test]
fn every_format_includes_every_entity() {
    for format in FORMATS {
        let out = render(&["--format", format]);
        for entity in DEMO_ENTITIES {
            assert!(
                out.contains(entity),
                "{format} output is missing entity {entity}"
            );
        }
    }
}

#[test]
fn no_attributes_drops_attribute_lines() {
    // with attributes the dot node carries a record label with a body; without
    // them it collapses to a bare label.
    let with_attrs = render(&["--format", "dot"]);
    assert!(with_attrs.contains("\"InfraDevice\" [label=\"{InfraDevice|"));

    let without = render(&["--format", "dot", "--no-attributes"]);
    assert!(without.contains("\"InfraDevice\" [label=\"InfraDevice\"]"));
    assert!(!without.contains("\"InfraDevice\" [label=\"{"));
}

#[test]
fn no_attributes_drops_attribute_types_in_every_format() {
    // TextAttribute is the declared type of many entity attributes, so it shows
    // up in every format's attribute lines — and must disappear entirely under
    // --no-attributes, not just in dot.
    for format in FORMATS {
        let with_attrs = render(&["--format", format]);
        assert!(
            with_attrs.contains("TextAttribute"),
            "{format} output should list attribute types when attributes are shown"
        );
        let without = render(&["--format", format, "--no-attributes"]);
        assert!(
            !without.contains("TextAttribute"),
            "{format} output should drop attribute types under --no-attributes"
        );
    }
}

#[test]
fn include_filter_keeps_only_matching_entities() {
    let out = render(&["--include", "^Infra"]);
    assert!(out.contains("InfraDevice"));
    assert!(out.contains("InfraVLAN"));
    // non-matching entities are filtered out entirely, edges included.
    assert!(!out.contains("LocationSite"));
    assert!(!out.contains("BuiltinTag"));
}

#[test]
fn exclude_filter_removes_matching_entities() {
    let out = render(&["--exclude", "Location"]);
    assert!(out.contains("InfraDevice"));
    // both Location* entities are gone, along with any edges to them.
    assert!(!out.contains("LocationSite"));
    assert!(!out.contains("LocationRack"));
}

#[test]
fn include_then_exclude_are_applied_in_order() {
    // include runs first, then exclude: `^Infra` keeps the five Infra* entities,
    // then `Interface` drops InfraInterface while the rest survive.
    let out = render(&["--include", "^Infra", "--exclude", "Interface"]);
    assert!(out.contains("InfraDevice"));
    assert!(out.contains("InfraVLAN"));
    // dropped by --exclude even though it matched the --include pattern.
    assert!(!out.contains("InfraInterface"));
    // never matched --include in the first place, so absent regardless.
    assert!(!out.contains("LocationSite"));
    assert!(!out.contains("BuiltinTag"));
}

#[test]
fn output_flag_writes_to_file_not_stdout() {
    let path =
        std::env::temp_dir().join(format!("infrahub-erd-cli-test-{}.dot", std::process::id()));
    let schema = demo_schema();
    let out = run(&[
        "--schema-file",
        schema.to_str().unwrap(),
        "--output",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    // nothing is printed to stdout when --output is set.
    assert!(out.stdout.is_empty());

    let written = std::fs::read_to_string(&path).expect("output file was not written");
    assert!(written.starts_with("digraph schema {"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn output_flag_writes_non_dot_format_to_file() {
    // --output is not dot-specific: a non-default format is written to the file
    // and stdout stays empty just the same.
    let path =
        std::env::temp_dir().join(format!("infrahub-erd-cli-test-{}.mmd", std::process::id()));
    let schema = demo_schema();
    let out = run(&[
        "--schema-file",
        schema.to_str().unwrap(),
        "--format",
        "mermaid",
        "--output",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());

    let written = std::fs::read_to_string(&path).expect("output file was not written");
    assert!(written.starts_with("erDiagram"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_schema_source_is_an_error() {
    // no --schema-file and no --url: the cli must fail with a helpful message.
    let out = run(&[]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--url or INFRAHUB_URL required"));
}

#[test]
fn nonexistent_schema_file_is_an_error() {
    let out = run(&["--schema-file", "/no/such/infrahub-erd/schema.graphql"]);
    assert!(!out.status.success());
    assert!(!out.stderr.is_empty());
}

#[test]
fn invalid_format_value_is_rejected() {
    let schema = demo_schema();
    let out = run(&[
        "--schema-file",
        schema.to_str().unwrap(),
        "--format",
        "bogus",
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid value 'bogus'"));
    // the error lists the accepted, kebab-cased values.
    assert!(stderr.contains("[possible values: dot, mermaid, plant-uml, d2]"));
}

#[test]
fn plantuml_must_be_kebab_cased() {
    let schema = demo_schema();
    // the accepted spelling is `plant-uml`...
    assert!(render(&["--format", "plant-uml"]).starts_with("@startuml"));
    // ...and the un-hyphenated `plantuml` is not a valid value.
    let out = run(&[
        "--schema-file",
        schema.to_str().unwrap(),
        "--format",
        "plantuml",
    ]);
    assert!(!out.status.success());
}

#[test]
fn short_f_selects_schema_file_not_format() {
    // `-f` is --schema-file; --format has no short flag, so the two never
    // collide even when both appear.
    let schema = demo_schema();
    let out = run(&["-f", schema.to_str().unwrap(), "--format", "mermaid"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("erDiagram"));
}

fn generic_schema() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generic-schema.graphql")
}

/// run the binary against the interface-bearing fixture and return its stdout.
fn render_generic(extra: &[&str]) -> String {
    let schema = generic_schema();
    let mut args = vec!["--schema-file", schema.to_str().unwrap()];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(
        out.status.success(),
        "expected success for args {extra:?}, got {:?}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout was not utf-8")
}

#[test]
fn generics_reach_every_format() {
    for format in FORMATS {
        let out = render_generic(&["--format", format]);
        for entity in ["InfraDevice", "CoreGroup", "CoreNode"] {
            assert!(out.contains(entity), "{format} output is missing {entity}");
        }
        assert!(
            out.contains("primary_group"),
            "{format} output dropped the relationship to a generic"
        );
        assert!(
            out.contains("members"),
            "{format} output dropped the generic's own relationship"
        );
        // universal boilerplate stays out so no generic becomes a hub node
        assert!(
            !out.contains("member_of_groups") && !out.contains("subscriber_of_groups"),
            "{format} output rendered universal group membership"
        );
        assert!(
            !out.contains("profiles"),
            "{format} output rendered profiles"
        );
        // a generic nothing points at is not drawn
        assert!(
            !out.contains("CoreUnreferenced"),
            "{format} output drew an unreferenced generic"
        );
        // AttributeInterface implementors stay attribute wrappers
        assert!(
            !out.contains("AttributeInterface"),
            "{format} output drew AttributeInterface"
        );
    }
}

#[test]
fn exclude_prunes_relationships_to_a_generic() {
    let out = render_generic(&["--exclude", "^CoreGroup$"]);
    assert!(out.contains("InfraDevice"));
    assert!(!out.contains("CoreGroup"));
    // the edge into the excluded generic goes with it
    assert!(!out.contains("primary_group"));
}

/// how each format spells `CoreRepository` implementing `CoreGenericRepository`.
const INHERITANCE_EDGES: &[(&str, &str)] = &[
    (
        "dot",
        r#""CoreRepository" -> "CoreGenericRepository" [arrowhead=onormal, style=dashed];"#,
    ),
    (
        "mermaid",
        r#""CoreRepository" }o..|| "CoreGenericRepository" : "is a""#,
    ),
    (
        "plant-uml",
        r#""CoreRepository" --|> "CoreGenericRepository""#,
    ),
    ("d2", "CoreRepository -> CoreGenericRepository: is a {"),
];

/// how each format would spell an `InfraDevice` inheriting the `CoreNode`
/// node interface, which no format may draw.
const NODE_INTERFACE_EDGES: &[(&str, &str)] = &[
    ("dot", r#""InfraDevice" -> "CoreNode" [arrowhead=onormal"#),
    ("mermaid", r#""InfraDevice" }o..|| "CoreNode""#),
    ("plant-uml", r#""InfraDevice" --|> "CoreNode""#),
    ("d2", "InfraDevice -> CoreNode: is a"),
];

#[test]
fn inheritance_edges_reach_every_format() {
    for (format, edge) in INHERITANCE_EDGES {
        let out = render_generic(&["--format", format]);
        assert!(
            out.contains(edge),
            "{format} output is missing the inheritance edge {edge}"
        );
    }
}

#[test]
fn inheritance_edges_survive_no_attributes() {
    for (format, edge) in INHERITANCE_EDGES {
        let out = render_generic(&["--format", format, "--no-attributes"]);
        assert!(
            out.contains(edge),
            "{format} output dropped the inheritance edge under --no-attributes"
        );
    }
}

#[test]
fn node_interfaces_are_never_inheritance_edges() {
    for (format, edge) in NODE_INTERFACE_EDGES {
        let out = render_generic(&["--format", format]);
        assert!(
            !out.contains(edge),
            "{format} output drew every node as a child of CoreNode"
        );
    }
}

#[test]
fn exclude_prunes_inheritance_edges_to_a_generic() {
    for (format, edge) in INHERITANCE_EDGES {
        let out = render_generic(&["--format", format, "--exclude", "^CoreGenericRepository$"]);
        // the implementor survives; the generic and the edge to it do not
        assert!(out.contains("CoreRepository"));
        assert!(
            !out.contains("CoreGenericRepository"),
            "{format} kept the excluded generic"
        );
        assert!(
            !out.contains(edge),
            "{format} kept an inheritance edge into an excluded generic"
        );
    }
}
