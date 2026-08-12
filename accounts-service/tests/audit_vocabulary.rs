//! The emitted audit vocabulary must be a vocabulary audit-service accepts
//! (v1.18 PR 2a).
//!
//! Why this guard exists at all, and why it exists NOW rather than when the
//! emitters were written: `emit_audit` is fire-and-forget — it discards the
//! response and only `tracing::warn!`s a non-success status. `ingest_audit_event`
//! answers **422 `invalid entity_type` / `invalid action`** for anything outside
//! `VALID_ENTITY_TYPES` / `VALID_ACTIONS`. So a vocabulary disagreement between
//! the two sides is a defect with NO observable symptom anywhere in this repo,
//! in CI, or on `/health`: every request succeeds, every deploy is green, and
//! the audit trail is simply empty. While `AUDIT_SERVICE_URL` was unset that was
//! harmless, because nothing was posted at all. This increment puts the variable
//! in the deploy matrix, which makes the agreement load-bearing for the first
//! time.
//!
//! The guard reads BOTH sources from disk on every run (L-003) — audit-service's
//! constants and the emitters' own call sites — so it cannot be satisfied by a
//! copy of either. It is a source scan, and it is deliberately paired with
//! `tests/audit_emit.rs`, which drives the real function and observes the bytes
//! on the wire: this file answers "is the vocabulary legal", that file answers
//! "is a request made at all". Neither substitutes for the other.
//!
//! Floors follow L-090: the CORPUS floor (how many services were discovered)
//! sits at today's real size because the workspace membership changes loudly
//! and rarely; every CONTENT floor (names parsed out of that corpus) sits at
//! "read nothing", so an ordinary vocabulary change is REPORTED as a finding
//! rather than aborted as a broken guard.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Corpus floor: workspace members with a service source tree.
const MIN_SERVICES: usize = 11;
/// Content floors: refuse only when a parse read NOTHING.
const MIN_EMITTER_FILES: usize = 1;
const MIN_EMISSIONS: usize = 1;
const MIN_VOCABULARY_VALUES: usize = 1;

/// The three lifecycle actions every emitter's spec claims it records.
const LIFECYCLE_ACTIONS: &[&str] = &["created", "updated", "deleted"];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must live inside the workspace root")
        .to_path_buf()
}

/// Every `*-service/src/lib/handlers` directory, glob-discovered (L-031) rather
/// than hand-listed, so a new service joins this guard's corpus by existing.
fn service_handler_dirs() -> BTreeMap<String, PathBuf> {
    let root = workspace_root();
    let mut found = BTreeMap::new();
    for entry in fs::read_dir(&root).expect("could not read the workspace root") {
        let entry = entry.expect("could not read a workspace root entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with("-service") {
            continue;
        }
        let handlers = entry.path().join("src").join("lib").join("handlers");
        if handlers.is_dir() {
            found.insert(name, handlers);
        }
    }
    found
}

/// Collects the string literals inside the balanced delimiter pair that begins
/// at `open`, in source order, stopping the moment THAT delimiter closes.
///
/// The delimiter is read from the byte at `open` rather than assumed, and that
/// is not defensive decoration — it is the fix for a defect this guard's own
/// negative control found. The first draft tracked `(`/`)` only and was handed
/// the `[` of `const VALID_ENTITY_TYPES: &[&str] = &[...]`, so it never saw a
/// depth change until the `#[derive(...)]` two lines below: it silently
/// returned SEVEN values — the four entity types PLUS the three actions of the
/// next constant — and an emitter sending `entity_type: "created"` would have
/// passed. Control D is what surfaced it, by printing the parsed set beside
/// the rejected value instead of only the verdict.
fn string_literals_in_delimited(bytes: &[u8], open: usize) -> Vec<String> {
    let (opener, closer) = match bytes.get(open) {
        Some(b'(') => (b'(', b')'),
        Some(b'[') => (b'[', b']'),
        other => panic!(
            "CANNOT-READ: expected `(` or `[` at byte {open}, found {:?}. Fix this \
             guard, do not delete it.",
            other.map(|b| *b as char)
        ),
    };

    let mut literals = Vec::new();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b if b == opener => depth += 1,
            b if b == closer => {
                depth -= 1;
                if depth == 0 {
                    return literals;
                }
            }
            b'"' => {
                let mut value = String::new();
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    if bytes[index] == b'\\' {
                        index += 1;
                        if index >= bytes.len() {
                            break;
                        }
                    }
                    value.push(bytes[index] as char);
                    index += 1;
                }
                literals.push(value);
            }
            _ => {}
        }
        index += 1;
    }
    panic!(
        "CANNOT-READ: the delimiter opened at byte {open} is never closed. \
         Fix this guard, do not delete it."
    );
}

/// Every `(entity_type, action)` pair the emitters actually pass, per service.
///
/// `emit_audit`'s signature is
/// `(client, entity_type, entity_id, action, actor_id, entity_label, auth_header)`,
/// and at every call site the only string literals are `entity_type`, `action`
/// and — where the entity carries no label — nothing else, because ids, labels
/// and headers are all bound variables. So the FIRST TWO literals inside the
/// call are exactly the two the vocabulary governs. The scan asserts that shape
/// instead of assuming it: a call site with fewer than two literals is a
/// CANNOT-READ refusal (L-049), never a silently skipped row.
fn emissions() -> BTreeMap<String, Vec<(String, String)>> {
    let mut per_service: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for (service, dir) in service_handler_dirs() {
        for entry in fs::read_dir(&dir).expect("could not read a handlers directory") {
            let path = entry.expect("could not read a handler entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = read_source(&path);
            let bytes = source.as_bytes();
            let mut cursor = 0usize;
            while let Some(offset) = source[cursor..].find("emit_audit(") {
                let at = cursor + offset;
                let open = at + "emit_audit".len();
                cursor = open;

                // Skip the definition itself: only call sites carry literals.
                let line_start = source[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
                if source[line_start..at].contains("fn ") {
                    continue;
                }

                let literals = string_literals_in_delimited(bytes, open);
                assert!(
                    literals.len() >= 2,
                    "CANNOT-READ: the `emit_audit(` call site at {} byte {at} yielded \
                     {} string literal(s); this guard reads entity_type and action as \
                     the first two literals of the call, so the call shape changed. \
                     Fix this guard, do not delete it.",
                    path.display(),
                    literals.len()
                );
                per_service
                    .entry(service.clone())
                    .or_default()
                    .push((literals[0].clone(), literals[1].clone()));
            }
        }
    }

    per_service
}

/// Reads a `const NAME: &[&str] = &[...]` array out of audit-service's models.
fn vocabulary(name: &str) -> BTreeSet<String> {
    let path = workspace_root()
        .join("audit-service")
        .join("src")
        .join("lib")
        .join("models.rs");
    let source = read_source(&path);
    let anchor = format!("const {name}: &[&str] = &[");
    let start = source.find(&anchor).unwrap_or_else(|| {
        panic!(
            "CANNOT-READ: {} declares no `{anchor}`; audit-service's vocabulary moved \
             or was renamed. Fix this guard, do not delete it.",
            path.display()
        )
    });
    let open = start + anchor.len() - 1;
    let values: BTreeSet<String> = string_literals_in_delimited(source.as_bytes(), open)
        .into_iter()
        .map(|v| v.to_lowercase())
        .collect();
    assert!(
        values.len() >= MIN_VOCABULARY_VALUES,
        "CANNOT-READ: parsed {} value(s) out of {name}, expected at least \
         {MIN_VOCABULARY_VALUES} — the parse read nothing. Fix this guard, do not \
         delete it.",
        values.len()
    );
    values
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("CANNOT-READ: {}: {err}", path.display()))
}

fn corpus_checked_emissions() -> BTreeMap<String, Vec<(String, String)>> {
    let services = service_handler_dirs();
    assert!(
        services.len() >= MIN_SERVICES,
        "FATAL: discovered {} services with a src/lib/handlers directory, expected at \
         least {MIN_SERVICES} — refusing to report a verdict on a corpus this small. \
         Fix this guard, do not delete it.",
        services.len()
    );

    let per_service = emissions();
    assert!(
        per_service.len() >= MIN_EMITTER_FILES,
        "CANNOT-READ: found {} service(s) calling `emit_audit`, expected at least \
         {MIN_EMITTER_FILES} — the call-site parse read nothing across {} services. \
         Fix this guard, do not delete it.",
        per_service.len(),
        services.len()
    );
    let total: usize = per_service.values().map(Vec::len).sum();
    assert!(
        total >= MIN_EMISSIONS,
        "CANNOT-READ: parsed {total} emission(s), expected at least {MIN_EMISSIONS}. \
         Fix this guard, do not delete it."
    );
    per_service
}

#[test]
fn every_emitted_vocabulary_pair_is_accepted_by_audit_service() {
    let per_service = corpus_checked_emissions();
    let valid_entity_types = vocabulary("VALID_ENTITY_TYPES");
    let valid_actions = vocabulary("VALID_ACTIONS");

    let mut rejected: Vec<String> = Vec::new();
    for (service, pairs) in &per_service {
        for (entity_type, action) in pairs {
            if !valid_entity_types.contains(&entity_type.to_lowercase()) {
                rejected.push(format!(
                    "{service} emits entity_type {entity_type:?}, which audit-service \
                     answers 422 `invalid entity_type` for (valid: {valid_entity_types:?})"
                ));
            }
            if !valid_actions.contains(&action.to_lowercase()) {
                rejected.push(format!(
                    "{service} emits action {action:?}, which audit-service answers 422 \
                     `invalid action` for (valid: {valid_actions:?})"
                ));
            }
        }
    }

    assert!(
        rejected.is_empty(),
        "{} emission(s) would be rejected by audit-service and discarded silently, \
         because `emit_audit` ignores the response:\n  {}",
        rejected.len(),
        rejected.join("\n  ")
    );
}

/// Regression pin for the defect control D found in this guard's first draft.
///
/// `VALID_ENTITY_TYPES` and `VALID_ACTIONS` are declared on consecutive lines,
/// so a scanner that does not stop at the array's own closing delimiter reads
/// straight through the first constant into the second and reports their UNION
/// as the entity-type vocabulary — which silently admits `entity_type:
/// "created"`. An overlap between the two parsed sets is that failure and
/// nothing else: no entity type in this domain is also a lifecycle action.
/// Asserting disjointness catches it without hand-copying either list, which
/// would reintroduce exactly the drift this file exists to prevent.
#[test]
fn the_two_vocabularies_are_read_as_separate_sets() {
    let entity_types = vocabulary("VALID_ENTITY_TYPES");
    let actions = vocabulary("VALID_ACTIONS");

    let overlap: Vec<&String> = entity_types.intersection(&actions).collect();
    assert!(
        overlap.is_empty(),
        "the parsed vocabularies overlap on {overlap:?}, which means the array scan \
         ran past one constant into the next: entity types read as \
         {entity_types:?} and actions as {actions:?}. Fix the scan, not this \
         assertion."
    );
}

#[test]
fn every_emitter_covers_create_update_and_delete() {
    let per_service = corpus_checked_emissions();

    let mut gaps: Vec<String> = Vec::new();
    for (service, pairs) in &per_service {
        let actions: BTreeSet<String> = pairs.iter().map(|(_, a)| a.to_lowercase()).collect();
        for expected in LIFECYCLE_ACTIONS {
            if !actions.contains(*expected) {
                gaps.push(format!(
                    "{service} never emits the {expected:?} action (it emits {actions:?}), \
                     so its openapi.yaml's claim that create/update/delete are audited is \
                     false for that verb"
                ));
            }
        }
    }

    assert!(
        gaps.is_empty(),
        "{} lifecycle gap(s) in the audit trail:\n  {}",
        gaps.len(),
        gaps.join("\n  ")
    );
}
