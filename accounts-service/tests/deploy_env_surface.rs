//! v1.18 PR 1 — the deploy-config drift guard: DECLARED versus DEPLOYED.
//!
//! Every service reads its configuration from the environment, and the deploy
//! job in `.github/workflows/rust.yml` is the only thing that puts anything
//! there. Nothing has ever compared the two lists, and the gap is invisible in
//! the safe-looking direction: a variable the code reads and no deployment
//! sets does not crash anything, because every read in this workspace falls
//! back SILENTLY —
//!
//! ```text
//!   let Ok(url) = std::env::var("AUDIT_SERVICE_URL") else { return; };
//!   Err(_) => return true,                       // fail-open existence check
//!   std::env::var("ACCOUNTS_SERVICE_URL").unwrap_or_default()
//!   env::var("DATABASE_REPLICA_URL").ok()
//! ```
//!
//! — so the service boots, `/health` returns 200, the PR #111 post-deploy
//! smoke test passes, all eleven `Deploy` jobs go green, and a documented
//! capability quietly never runs. SEARCH-WRITETHROUGH-INERT-1 is that exact
//! outcome already in production: five services push CRM entities into the
//! search index on every create/update/delete, `SEARCH_SERVICE_URL` has never
//! been deployed, and the index has therefore never been written to once.
//!
//! This guard does not fix that. It makes it impossible for the next one to
//! arrive unnoticed: every `env::var("...")` read in `*-service/src/**` must
//! be either supplied by the deploy job, provided by the Cloud Run runtime, or
//! ALLOW-LISTED HERE WITH A REASON. The allow-list is the declared-versus-
//! deployed delta, written down, in one place, with the consequence of each
//! gap spelled out — twelve variables today.
//!
//! **The two sides come from two different artifacts on purpose** (L-054): the
//! read set is lexed out of Rust source, the supply set is parsed out of the
//! workflow yaml, and the allow-list is a third hand-written source. No single
//! perturbation can move two of them at once, which is what makes the negative
//! controls falsifiable — adding `env::var("MADE_UP_VAR")` to a service moves
//! only the Rust side, deleting a name from `--set-env-vars` moves only the
//! yaml side, and supplying an allow-listed variable moves neither but leaves
//! a dead allow-list entry that `no_allow_list_entry_is_dead` reports.
//!
//! Both scans are GLOB-DISCOVERED with hard refusals rather than hand lists
//! (L-031), so a twelfth service joins the guard with no edit here, and a
//! parse that silently reads nothing FAILS instead of reporting clean (L-049).
//!
//! The guard lives in accounts-service only because a Rust test must live in
//! some crate; it deliberately scans the workspace root and needs no database.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Floors. Every one of these exists so that a parse which silently matched
/// nothing REFUSES to report a verdict instead of reporting a clean one.
///
/// **They are calibrated to catch a BROKEN PARSER, never a changed config, and
/// the difference is not cosmetic.** The corpus floors below sit at 11 because
/// eleven services and eleven matrix entries are a structural property of this
/// workspace: a twelfth is additive, and losing one is a shape change that
/// `the_deploy_matrix_covers_every_workspace_service` reports by name. The
/// SUPPLY floors deliberately sit at 1 instead of at today's counts (3 base
/// env names, 2 base secrets), because a floor pinned to today's count turns
/// an ordinary config regression into a CANNOT-READ refusal that blames the
/// guard: deleting `AUTH_ISSUER=auth-service#` from `--set-env-vars` made this
/// file's first draft answer "parsed only 2 names, fix this guard" on all
/// three tests, when the true finding is that eleven services now read a
/// variable nothing supplies. Refuse when the parse read NOTHING; report when
/// it read something different.
const MIN_SERVICES: usize = 11;
const MIN_MATRIX_ENTRIES: usize = 11;
const MIN_BASE_ENV_NAMES: usize = 1;
const MIN_BASE_SECRET_NAMES: usize = 1;

/// Names the Cloud Run RUNTIME provides, so the deploy step is right not to
/// set them and they are not part of the declared-versus-deployed delta.
const PLATFORM_PROVIDED: &[(&str, &str)] = &[
    (
        "PORT",
        "Cloud Run injects PORT into every container, and the deploy step pins \
         `--port 8080` to match each service's own 8080 fallback.",
    ),
    (
        "HOST",
        "no deployment sets a bind address; every service's main.rs defaults to \
         0.0.0.0, which is what Cloud Run requires a container to listen on.",
    ),
];

/// The declared-versus-deployed delta: variables the code reads that no
/// deployment supplies, each with the CONSEQUENCE of the gap rather than a
/// label. An entry here is a decision that the silent fallback is acceptable
/// today; deleting the entry is what v1.18 PR 2 does when it wires the
/// variable into the deploy matrix.
///
/// Keep this sorted by name. Eleven entries today: `AUDIT_SERVICE_URL` left on
/// 2026-08-12 when v1.18 PR 2a put it in the deploy matrix, which is exactly the
/// event `no_allow_list_entry_is_dead` exists to force. Its retired text, kept
/// here because the reason is the record of what the gap WAS: "the four CRM
/// emitters (accounts, activities, contacts, opportunities) return from
/// `emit_audit_event` at the first `env::var` miss, so no audit event has ever
/// been emitted in production. Wired by v1.18 PR 2."
const ALLOWED_UNSUPPLIED: &[(&str, &str)] = &[
    (
        "ACCOUNTS_SERVICE_URL",
        "activities/contacts verify a referenced account exists and FAIL OPEN when \
         unset, so the 400 'referenced account does not exist' their specs document \
         cannot fire; reporting folds accounts into a rollup and omits it when unset. \
         Wired by v1.18 PR 2b, which is a deliberate production behaviour change.",
    ),
    (
        "ACTIVITIES_SERVICE_URL",
        "reporting-service reads it with `.unwrap_or_default()` for the cross-service \
         rollup, so an unset value silently drops that section rather than erroring. \
         Wired by v1.18 PR 2b.",
    ),
    (
        "AUTH_JWT_PUBLIC_KEY",
        "read only by the RS256/RS384/RS512 arm of `decoding_key`; the deploy pins \
         AUTH_JWT_ALGORITHM=HS256, so that arm is unreachable in production and the \
         variable is correctly absent. Supplying it would be the error.",
    ),
    (
        "CONTACTS_SERVICE_URL",
        "activities-service verifies a referenced contact exists and FAILS OPEN when \
         unset; reporting-service folds contacts into a rollup. Wired by v1.18 PR 2b.",
    ),
    (
        "DATABASE_REPLICA_URL",
        "reporting and search take `.ok()` and fall back to the primary pool when \
         unset. No read replica exists on the Cloud SQL instance, so unset is the \
         correct state; this entry exists so that adding a replica is a deliberate \
         deploy edit rather than a silently ignored one.",
    ),
    (
        "OBSERVABOARD_API_KEY",
        "audit-service forwards events to the observaboard sibling, which lives in \
         its own repository and is not deployed from here (the same decision compose \
         records under COMPOSE-GAP-1). Unset means no forwarding.",
    ),
    (
        "OBSERVABOARD_INGEST_URL",
        "the endpoint half of the same observaboard forwarding; unset means audit \
         events stay in this platform's own database.",
    ),
    (
        "OPPORTUNITIES_SERVICE_URL",
        "reporting-service reads it with `.unwrap_or_default()` for the rollup, so an \
         unset value drops that section silently. Wired by v1.18 PR 2b.",
    ),
    (
        "PIPELINE_INGEST_URL",
        "five services post entity-change events to an analytics pipeline that was \
         never rebuilt after the 2026-06-04 decommission; `Err(_) => return` means \
         every emit is a no-op. Deliberately OUT of v1.18 PR 2a and 2b — there is no ingest \
         endpoint to point it at, so wiring it needs a destination decision first.",
    ),
    (
        "SEARCH_SERVICE_TOKEN",
        "the credential half of the write-through path below. It cannot simply be \
         added to the deploy matrix: it needs a token whose roles claim carries \
         `admin` or `service`, or the gate PR #137 shipped answers 403 to a \
         fire-and-forget caller that ignores the response. See \
         SEARCH-WRITETHROUGH-INERT-1.",
    ),
    (
        "SEARCH_SERVICE_URL",
        "ten services index and delete search documents fire-and-forget on entity \
         change, and every one of them returns at this `env::var` miss, so the search \
         index has never been written to in production. See \
         SEARCH-WRITETHROUGH-INERT-1: closing it also needs a backfill decision.",
    ),
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf()
}

fn read(path: &Path, label: &str) -> String {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("CANNOT-READ: {label} ({}): {err}", path.display()));
    assert!(!text.trim().is_empty(), "CANNOT-READ: {label} is empty");
    text
}

// ---------------------------------------------------------------------------
// Side A: what the Rust source READS.
// ---------------------------------------------------------------------------

/// Collects every `*.rs` under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("CANNOT-READ: directory {}: {err}", dir.display()));
    for entry in entries {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out.sort();
}

/// Extracts the argument of every `env::var("NAME")` call in CODE position.
///
/// This is a small lexer rather than a regex, because a regex cannot tell a
/// real call from one quoted inside a doc comment or a string literal, and a
/// guard that reports garbage gets deleted (L-031). It tracks line comments,
/// nested block comments, string literals and raw string literals, and it
/// REFUSES on the two constructs it does not model rather than guessing.
fn env_reads(source: &str, label: &str) -> BTreeSet<String> {
    for forbidden in ["'\"'", "'\\\\'"] {
        assert!(
            !source.contains(forbidden),
            "CANNOT-READ: {label} now contains the char literal {forbidden}, which \
             this guard's lexer would misread as a string delimiter. Teach it, do \
             not delete it."
        );
    }
    assert!(
        !source.contains("br#") && !source.contains("br\""),
        "CANNOT-READ: {label} now contains a byte-string literal, which this \
         guard's lexer does not model. Teach it, do not delete it."
    );

    let chars: Vec<char> = source.chars().collect();
    let mut names = BTreeSet::new();
    let mut i = 0usize;
    // Set the moment `env::var(` is consumed; the very next token must then be
    // a string literal, or this guard refuses.
    let mut expecting_literal = false;

    while i < chars.len() {
        // Line comment.
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Block comment (Rust nests them).
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            let mut depth = 1usize;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Raw string literal: r"..." / r#"..."# / r##"..."## ...
        if chars[i] == 'r'
            && !matches!(chars.get(i.wrapping_sub(1)), Some(c) if c.is_alphanumeric() || *c == '_')
        {
            let mut hashes = 0usize;
            let mut j = i + 1;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                let (content, next) = consume_raw_string(&chars, j + 1, hashes, label);
                if expecting_literal {
                    record(&mut names, &content, label);
                    expecting_literal = false;
                }
                i = next;
                continue;
            }
        }
        // Ordinary string literal.
        if chars[i] == '"' {
            let (content, next) = consume_string(&chars, i + 1, label);
            if expecting_literal {
                record(&mut names, &content, label);
                expecting_literal = false;
            }
            i = next;
            continue;
        }
        if expecting_literal {
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }
            panic!(
                "CANNOT-READ: {label} calls env::var with a non-literal argument \
                 (found `{}` where a string literal was expected). This guard \
                 cannot know which variable that reads, so it refuses to report a \
                 verdict. Teach it, do not delete it.",
                chars[i]
            );
        }
        if starts_with(&chars, i, "env::var(") {
            i += "env::var(".len();
            expecting_literal = true;
            continue;
        }
        i += 1;
    }

    assert!(
        !expecting_literal,
        "CANNOT-READ: {label} ends inside an unterminated env::var( call"
    );
    names
}

fn record(names: &mut BTreeSet<String>, content: &str, label: &str) {
    assert!(
        !content.is_empty()
            && content
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && content.starts_with(|c: char| c.is_ascii_uppercase()),
        "CANNOT-READ: {label} reads env::var(\"{content}\"), which is not a \
         SCREAMING_SNAKE_CASE variable name; this guard assumes deploy config is \
         named that way. Teach it, do not delete it."
    );
    names.insert(content.to_string());
}

fn starts_with(chars: &[char], at: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, c)| chars.get(at + offset) == Some(&c))
}

/// Consumes an ordinary string literal starting just past its opening quote.
/// Returns its content and the index just past the closing quote.
fn consume_string(chars: &[char], mut i: usize, label: &str) -> (String, usize) {
    let mut content = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                content.push(chars[i]);
                if let Some(next) = chars.get(i + 1) {
                    content.push(*next);
                }
                i += 2;
            }
            '"' => return (content, i + 1),
            c => {
                content.push(c);
                i += 1;
            }
        }
    }
    panic!("CANNOT-READ: {label} has an unterminated string literal");
}

/// Consumes a raw string literal body, which ends at `"` followed by exactly
/// the same number of `#` the opener used.
fn consume_raw_string(chars: &[char], mut i: usize, hashes: usize, label: &str) -> (String, usize) {
    let mut content = String::new();
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|n| chars.get(i + n) == Some(&'#')) {
            return (content, i + 1 + hashes);
        }
        content.push(chars[i]);
        i += 1;
    }
    panic!("CANNOT-READ: {label} has an unterminated raw string literal");
}

/// service directory -> the variables its `src/**` reads. Glob-discovered.
fn service_env_reads() -> BTreeMap<String, BTreeSet<String>> {
    let root = workspace_root();
    let entries = fs::read_dir(&root)
        .unwrap_or_else(|err| panic!("CANNOT-READ: workspace root {}: {err}", root.display()));

    let mut per_service: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let entry = entry.expect("read workspace dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with("-service") {
            continue;
        }
        let src = entry.path().join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(
            !files.is_empty(),
            "CANNOT-READ: {name}/src contains no .rs files"
        );
        let mut names = BTreeSet::new();
        for file in files {
            let label = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .display()
                .to_string();
            names.extend(env_reads(&read(&file, &label), &label));
        }
        per_service.insert(name, names);
    }
    per_service
}

// ---------------------------------------------------------------------------
// Side B: what the deploy job SUPPLIES.
// ---------------------------------------------------------------------------

/// The `deploy:` job block of rust.yml, as raw lines.
fn deploy_job_lines(workflow: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in workflow.lines() {
        let is_job_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-')
            && !line.trim_start().starts_with('#');
        if is_job_key {
            inside = line.trim() == "deploy:";
            continue;
        }
        if inside {
            lines.push(line.to_string());
        }
    }
    assert!(
        !lines.is_empty(),
        "CANNOT-READ: rust.yml has no `deploy:` job block this guard can find; \
         did the workflow change shape? Teach this guard, do not delete it."
    );
    lines
}

/// Removes every `${{ ... }}` expression, whose contents carry `#`, `=` and
/// `,` characters that would otherwise be parsed as configuration names.
fn strip_expressions(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let end = after.find("}}").unwrap_or_else(|| {
            panic!("CANNOT-READ: rust.yml has an unterminated ${{{{ }}}} expression")
        });
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The double-quoted argument of `flag` on the one line that carries it.
fn quoted_flag_argument(lines: &[String], flag: &str) -> String {
    let matches: Vec<&String> = lines.iter().filter(|line| line.contains(flag)).collect();
    assert_eq!(
        matches.len(),
        1,
        "CANNOT-READ: expected exactly one `{flag}` line in the deploy job, \
         found {}. Two would mean two supply sets this guard reads as one.",
        matches.len()
    );
    let line = matches[0];
    let after = line
        .split_once(flag)
        .map(|(_, rest)| rest.trim())
        .unwrap_or_default();
    let start = after
        .find('"')
        .unwrap_or_else(|| panic!("CANNOT-READ: `{flag}` argument is not double-quoted"));
    let end = after
        .rfind('"')
        .filter(|end| *end > start)
        .unwrap_or_else(|| panic!("CANNOT-READ: `{flag}` argument has no closing quote"));
    after[start + 1..end].to_string()
}

/// Splits a `NAME=value` list on `separator` and returns the NAMEs.
fn names_in_list(list: &str, separator: char) -> BTreeSet<String> {
    list.split(separator)
        .filter_map(|entry| entry.split_once('='))
        .map(|(name, _)| name.trim().to_string())
        .filter(|name| {
            !name.is_empty()
                && name.starts_with(|c: char| c.is_ascii_uppercase())
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .collect()
}

/// gcloud's `^<delim>^` prefix selects a custom list delimiter. Returns the
/// delimiter and the list with the prefix removed.
fn list_delimiter(list: &str) -> (char, String) {
    let chars: Vec<char> = list.chars().take(3).collect();
    if chars.len() == 3 && chars[0] == '^' && chars[2] == '^' {
        let prefix = chars[0].len_utf8() + chars[1].len_utf8() + chars[2].len_utf8();
        (chars[1], list[prefix..].to_string())
    } else {
        (',', list.to_string())
    }
}

/// The value of a `key:` line in one matrix entry, unquoted.
fn entry_value(entry_lines: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    entry_lines
        .iter()
        .map(|line| line.trim_start().trim_start_matches("- "))
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| {
            value
                .trim()
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string()
        })
}

/// service name -> every variable the deploy job puts in that container.
fn deploy_supply() -> BTreeMap<String, BTreeSet<String>> {
    let root = workspace_root();
    let workflow_path = root.join(".github").join("workflows").join("rust.yml");
    let workflow = read(&workflow_path, ".github/workflows/rust.yml");
    let lines = deploy_job_lines(&workflow);

    // The delimiters the matrix extras are concatenated with are DERIVED from
    // the workflow's own `format(...)` calls, never assumed.
    let env_line = lines
        .iter()
        .find(|line| line.contains("--set-env-vars"))
        .expect("CANNOT-READ: the deploy job has no --set-env-vars line");
    let secrets_line = lines
        .iter()
        .find(|line| line.contains("--set-secrets"))
        .expect("CANNOT-READ: the deploy job has no --set-secrets line");
    assert!(
        env_line.contains("matrix.extra_env"),
        "CANNOT-READ: --set-env-vars no longer appends matrix.extra_env; this \
         guard would miss every per-service extra. Teach it, do not delete it."
    );
    assert!(
        secrets_line.contains("matrix.extra_secrets"),
        "CANNOT-READ: --set-secrets no longer appends matrix.extra_secrets; this \
         guard would miss every per-service extra. Teach it, do not delete it."
    );

    let base_env_raw = quoted_flag_argument(&lines, "--set-env-vars");
    let (env_delimiter, base_env_list) = list_delimiter(&strip_expressions(&base_env_raw));
    let base_env = names_in_list(&base_env_list, env_delimiter);
    assert!(
        base_env.len() >= MIN_BASE_ENV_NAMES,
        "FATAL: parsed {} names out of the deploy job's --set-env-vars argument, \
         expected at least {MIN_BASE_ENV_NAMES} — the flag is present but this \
         guard read nothing out of it, so the quoting or the `^delim^` prefix \
         changed shape and every service would look under-configured. Fix this \
         guard, do not delete it.",
        base_env.len()
    );

    let base_secrets_raw = quoted_flag_argument(&lines, "--set-secrets");
    let base_secrets = names_in_list(&strip_expressions(&base_secrets_raw), ',');
    assert!(
        base_secrets.len() >= MIN_BASE_SECRET_NAMES,
        "FATAL: parsed {} names out of the deploy job's --set-secrets argument, \
         expected at least {MIN_BASE_SECRET_NAMES} — the flag is present but this \
         guard read nothing out of it, so its quoting changed shape. Fix this \
         guard, do not delete it.",
        base_secrets.len()
    );

    // Matrix entries: each starts at a `- service:` line and runs to the next.
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    for line in &lines {
        let trimmed = line.trim_start();
        if let Some(service) = trimmed.strip_prefix("- service:") {
            entries.push((service.trim().to_string(), vec![line.clone()]));
        } else if let Some((_, current)) = entries.last_mut() {
            if trimmed.starts_with("- ") || !line.starts_with("          ") {
                continue;
            }
            current.push(line.clone());
        }
    }
    assert!(
        entries.len() >= MIN_MATRIX_ENTRIES,
        "FATAL: parsed only {} deploy matrix entries from rust.yml, expected at \
         least {MIN_MATRIX_ENTRIES} — refusing to report a verdict on a matrix \
         this small. Fix this guard, do not delete it.",
        entries.len()
    );

    let mut supply: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (service, entry_lines) in entries {
        let mut names = base_env.clone();
        names.extend(base_secrets.iter().cloned());
        if let Some(extra_env) = entry_value(&entry_lines, "extra_env") {
            names.extend(names_in_list(&extra_env, env_delimiter));
        }
        if let Some(extra_secrets) = entry_value(&entry_lines, "extra_secrets") {
            names.extend(names_in_list(&extra_secrets, ','));
        }
        assert!(
            supply.insert(service.clone(), names).is_none(),
            "CANNOT-READ: rust.yml's deploy matrix lists {service} twice"
        );
    }
    supply
}

// ---------------------------------------------------------------------------
// The three truths, each read from BOTH sources on every run.
// ---------------------------------------------------------------------------

fn allow_listed() -> BTreeMap<&'static str, &'static str> {
    ALLOWED_UNSUPPLIED.iter().copied().collect()
}

fn platform_provided() -> BTreeSet<&'static str> {
    PLATFORM_PROVIDED.iter().map(|(name, _)| *name).collect()
}

#[test]
fn every_service_env_read_is_supplied_or_allow_listed() {
    let reads = service_env_reads();
    assert!(
        reads.len() >= MIN_SERVICES,
        "FATAL: discovered {} services with a src/ directory, expected at least \
         {MIN_SERVICES} — refusing to report a verdict on a corpus this small. \
         The workspace layout changed; fix this guard, do not delete it.",
        reads.len()
    );

    let supply = deploy_supply();
    let allowed = allow_listed();
    let platform = platform_provided();

    let mut undeclared: Vec<String> = Vec::new();
    for (service, names) in &reads {
        let supplied = supply.get(service).unwrap_or_else(|| {
            panic!(
                "{service} reads deploy configuration but has no entry in rust.yml's \
                 deploy matrix, so nothing ever sets any of it"
            )
        });
        for name in names {
            if supplied.contains(name) || platform.contains(name.as_str()) {
                continue;
            }
            if allowed.contains_key(name.as_str()) {
                continue;
            }
            undeclared.push(format!("{service} reads {name}"));
        }
    }

    assert!(
        undeclared.is_empty(),
        "{} (service, variable) pair(s) are read by the code and supplied by no \
         deployment, and are not on this guard's allow-list:\n  {}\n\nEvery read \
         in this workspace falls back silently, so this does NOT crash anything — \
         the service boots, /health returns 200 and the deploy goes green while \
         the capability never runs (SEARCH-WRITETHROUGH-INERT-1 is that outcome \
         already shipped). Either add the variable to the deploy matrix in \
         .github/workflows/rust.yml, or add it to ALLOWED_UNSUPPLIED in this file \
         with the CONSEQUENCE of leaving it unset.",
        undeclared.len(),
        undeclared.join("\n  ")
    );
}

#[test]
fn no_allow_list_entry_is_dead() {
    let reads = service_env_reads();
    assert!(
        reads.len() >= MIN_SERVICES,
        "FATAL: discovered {} services with a src/ directory, expected at least \
         {MIN_SERVICES} — refusing to report a verdict on a corpus this small.",
        reads.len()
    );
    let supply = deploy_supply();
    let platform = platform_provided();

    // Every variable that is read somewhere and supplied to nobody who reads it.
    let mut unsupplied: BTreeSet<String> = BTreeSet::new();
    for (service, names) in &reads {
        let supplied = supply.get(service).cloned().unwrap_or_default();
        for name in names {
            if !supplied.contains(name) && !platform.contains(name.as_str()) {
                unsupplied.insert(name.clone());
            }
        }
    }

    let allowed: BTreeSet<String> = ALLOWED_UNSUPPLIED
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    let dead: Vec<&String> = allowed.difference(&unsupplied).collect();
    assert!(
        dead.is_empty(),
        "ALLOWED_UNSUPPLIED entr(ies) {dead:?} are dead: every service that reads \
         them is now supplied them by the deploy job, or nothing reads them at all. \
         An allow-list entry that cannot fire is a recorded exception to a gap that \
         no longer exists — DELETE it, which is exactly what wiring a variable into \
         the deploy matrix (v1.18 PR 2) is supposed to do here."
    );

    // The two lists answer different questions and must never overlap: a name
    // the runtime provides is not an accepted gap.
    let overlap: Vec<&&str> = PLATFORM_PROVIDED
        .iter()
        .map(|(name, _)| name)
        .filter(|name| allowed.contains(**name))
        .collect();
    assert!(
        overlap.is_empty(),
        "{overlap:?} appear in BOTH PLATFORM_PROVIDED and ALLOWED_UNSUPPLIED; a \
         variable the Cloud Run runtime supplies is not an accepted configuration \
         gap. Pick one home."
    );

    for (name, reason) in ALLOWED_UNSUPPLIED {
        assert!(
            reason.len() > 40,
            "ALLOWED_UNSUPPLIED entry {name} has no real reason recorded; the \
             allow-list is the written-down declared-versus-deployed delta, so an \
             entry without its consequence is just a silenced check"
        );
    }
}

#[test]
fn the_deploy_matrix_covers_every_workspace_service() {
    let reads = service_env_reads();
    let supply = deploy_supply();

    let services: BTreeSet<&String> = reads.keys().collect();
    let deployed: BTreeSet<&String> = supply.keys().collect();

    let never_deployed: Vec<&&String> = services.difference(&deployed).collect();
    assert!(
        never_deployed.is_empty(),
        "service(s) {never_deployed:?} exist in the workspace but are absent from \
         rust.yml's deploy matrix, so they are built and tested by CI and never \
         shipped — declared but not deployed, in the most literal sense"
    );

    let phantom: Vec<&&String> = deployed.difference(&services).collect();
    assert!(
        phantom.is_empty(),
        "rust.yml's deploy matrix names {phantom:?}, which is not a *-service \
         directory with a src/ tree in this workspace; the deploy would build an \
         image from a path that does not exist"
    );
}
