//! Drift guard for the `SpendRecord.source` vocabulary (VALID_SOURCES wiring).
//!
//! The vocabulary of record-provenance values lives in THREE homes that must
//! agree:
//!   1. code: `spend_service::models::VALID_SOURCES`, built from the named
//!      `SOURCE_*` constants every write site now uses (compiler-enforced,
//!      so a fourth "the write sites agree with the const" assertion would be
//!      tautological and is deliberately absent);
//!   2. spec, machine-readable: the `SpendRecord.source` schema `enum` in
//!      `openapi.yaml` (what the generated SDK types are derived from);
//!   3. spec, human-readable: the list `source` filter parameter's
//!      "Known values are ..." sentence in the same file.
//!
//! Each parse step is a hard failure on zero matches: a reworded spec must
//! redden this guard, never silently empty its corpus. No database is
//! required — this suite runs everywhere the crate compiles.
//!
//! Known scope limit, recorded rather than papered over: a write site that
//! hand-types a source literal instead of importing a `SOURCE_*` constant is
//! outside this guard's sight; the import wiring is what prevents that, and
//! review owns the rest.

use spend_service::models::{SOURCE_MANUAL, VALID_SOURCES};

const OPENAPI: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.yaml"));

/// Extracts the `enum` values of the `source` property inside the
/// `SpendRecord:` schema block. Structural anchors, each asserted to match
/// exactly once, so prose elsewhere in the file cannot satisfy the parse.
fn schema_source_enum() -> Vec<String> {
    let lines: Vec<&str> = OPENAPI.lines().collect();

    let schema_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_end() == "    SpendRecord:")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        schema_starts.len(),
        1,
        "expected exactly one `    SpendRecord:` schema heading in openapi.yaml, found {}",
        schema_starts.len()
    );
    let start = schema_starts[0];

    // The schema block ends at the next 4-space-indented key (the next schema).
    let end = lines[start + 1..]
        .iter()
        .position(|l| {
            l.starts_with("    ") && !l.starts_with("     ") && l.trim_end().ends_with(':')
        })
        .map(|p| start + 1 + p)
        .unwrap_or(lines.len());
    let block = &lines[start..end];

    let source_props: Vec<usize> = block
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_end() == "        source:")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        source_props.len(),
        1,
        "expected exactly one `source:` property in the SpendRecord schema, found {}",
        source_props.len()
    );
    let prop = source_props[0];

    let enum_line = block[prop..]
        .iter()
        .position(|l| l.trim_end() == "          enum:")
        .map(|p| prop + p)
        .expect("the SpendRecord `source` property must declare an `enum:` — if it was removed or reworded, update this guard AND tests below deliberately");

    let mut values = Vec::new();
    for line in &block[enum_line + 1..] {
        let trimmed = line.trim_start();
        if let Some(v) = trimmed.strip_prefix("- ") {
            values.push(v.trim_end().to_string());
        } else {
            break;
        }
    }
    assert!(
        !values.is_empty(),
        "parsed the SpendRecord source enum: but collected zero values — the guard's corpus emptied"
    );
    values
}

/// Extracts the enumeration from the list `source` query parameter's
/// "Known values are X, Y, ..., and Z, but ..." description sentence.
fn filter_param_known_values() -> Vec<String> {
    let param_anchors: Vec<usize> = OPENAPI
        .lines()
        .enumerate()
        .filter(|(_, l)| l.trim_end().ends_with("- name: source"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        param_anchors.len(),
        1,
        "expected exactly one `- name: source` parameter in openapi.yaml, found {}",
        param_anchors.len()
    );

    let lines: Vec<&str> = OPENAPI.lines().collect();
    let start = param_anchors[0];
    // Parameter blocks here are short; the description sentence sits within
    // the next ten lines. Join them and locate the sentence.
    let window = lines[start..(start + 10).min(lines.len())].join(" ");
    let after = window
        .split("Known values are ")
        .nth(1)
        .expect("the source filter description must carry the `Known values are ...` sentence — if reworded, update this guard deliberately");
    let sentence = after
        .split(", but")
        .next()
        .expect("split always yields at least one element");

    let values: Vec<String> = sentence
        .split(',')
        .map(|part| part.trim().trim_start_matches("and ").trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();
    assert!(
        !values.is_empty(),
        "parsed the `Known values are` sentence but collected zero values — the guard's corpus emptied"
    );
    values
}

fn as_sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn valid_sources_sorted() -> Vec<String> {
    as_sorted(VALID_SOURCES.iter().map(|s| s.to_string()).collect())
}

#[test]
fn valid_sources_is_nonempty_and_duplicate_free() {
    assert!(!VALID_SOURCES.is_empty(), "VALID_SOURCES emptied");
    let sorted = valid_sources_sorted();
    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(sorted, deduped, "VALID_SOURCES carries a duplicate value");
    assert!(
        VALID_SOURCES.contains(&SOURCE_MANUAL),
        "SOURCE_MANUAL must be a member: the update/delete record-source guard depends on it"
    );
}

#[test]
fn schema_enum_matches_valid_sources() {
    let spec_side = as_sorted(schema_source_enum());
    let code_side = valid_sources_sorted();
    assert_eq!(
        spec_side, code_side,
        "the SpendRecord.source schema enum in openapi.yaml and VALID_SOURCES in models.rs disagree — \
         a source was added, removed, or renamed in one home and not the other"
    );
}

#[test]
fn filter_param_prose_matches_valid_sources() {
    let spec_side = as_sorted(filter_param_known_values());
    let code_side = valid_sources_sorted();
    assert_eq!(
        spec_side, code_side,
        "the list `source` filter's `Known values are ...` sentence in openapi.yaml and VALID_SOURCES \
         in models.rs disagree — a source was added, removed, or renamed in one home and not the other"
    );
}
