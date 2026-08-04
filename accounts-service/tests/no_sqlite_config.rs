//! Workspace drift guard for the CONFIG layer, the sibling of
//! `no_sqlite_fallback.rs` (which guards the service binaries).
//!
//! Every workspace service compiles sqlx with postgres-only features, so a
//! sqlite URL in any config surface (docker-compose, a check script, a
//! deployment manifest) is dead configuration: it can never connect, and it
//! survived unnoticed for months precisely because nothing executed it.
//! PR #117 retired the class from the binaries; this guard retires it from
//! the config layer and keeps both truths from drifting:
//!
//! 1. No config surface references sqlite, and none references
//!    `TEST_DATABASE_URL` (grep-verified 2026-08-04: no Rust source and no
//!    workflow reads that variable, so any config setting it is dead).
//! 2. No `*-service/fly.toml` exists: the Fly apps were destroyed in the
//!    2026-06-04 teardown and every service deploys to Cloud Run, so a
//!    fly.toml can only mislead.
//! 3. The local-dev database list (scripts/postgres-init/, docker-compose,
//!    both run-checks scripts) agrees with the list CI creates in
//!    .github/workflows/rust.yml — read from BOTH sources on every run, so
//!    a change to either side reddens CI instead of drifting silently.
//!
//! The guard lives in accounts-service only because a Rust test must live in
//! some crate; it deliberately scans the workspace root.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf()
}

/// Recursively collect every regular file under `dir`.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display())) {
        let path = entry.expect("read dir entry").path();
        if path.is_dir() {
            files_under(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Literal database names taken from `postgres://.../<name>` URLs in `source`.
/// Script-variable segments (`$db`, `${db}`) are skipped: the literal names in
/// those files are validated through their crate->database tables instead.
fn postgres_database_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let mut rest = line;
        while let Some(pos) = rest.find("postgres://") {
            let tail = &rest[pos..];
            let end = tail
                .find(|c: char| c == '"' || c == '\'' || c.is_whitespace())
                .unwrap_or(tail.len());
            if let Some((_, db)) = tail[..end].rsplit_once('/') {
                if !db.is_empty() && !db.contains('$') && !db.contains('{') {
                    names.push(db.to_string());
                }
            }
            rest = &tail[end..];
        }
    }
    names
}

#[test]
fn config_surfaces_reference_no_dead_database_engine() {
    let root = workspace_root();
    let mut targets = vec![root.join("docker-compose.yml")];
    files_under(&root.join("scripts"), &mut targets);

    // Negative control: compose plus at least the two run-checks scripts and
    // the postgres-init file must be in the scan, or the scan saw nothing.
    assert!(
        targets.len() >= 4,
        "expected to scan at least 4 config files, scanned {}; \
         did the layout change? Update this guard, do not delete it.",
        targets.len()
    );

    let mut offenders = Vec::new();
    for path in &targets {
        let source = read(path);
        for (idx, line) in source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("sqlite") || lower.contains("test_database_url") {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.strip_prefix(&root).unwrap_or(path).display(),
                    idx + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "dead database config (services are postgres-only, and nothing reads \
         TEST_DATABASE_URL):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn no_fly_era_deployment_manifests() {
    let root = workspace_root();
    let mut service_dirs = 0usize;
    let mut offenders = Vec::new();

    for entry in fs::read_dir(&root).expect("read workspace root") {
        let entry = entry.expect("read workspace dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with("-service") || !entry.path().is_dir() {
            continue;
        }
        service_dirs += 1;
        if entry.path().join("fly.toml").is_file() {
            offenders.push(name);
        }
    }

    assert!(
        service_dirs >= 11,
        "expected at least 11 service directories, saw {service_dirs}; \
         did the workspace layout change? Update this guard, do not delete it."
    );

    assert!(
        offenders.is_empty(),
        "fly.toml present in {offenders:?}: the Fly apps were destroyed in the \
         2026-06-04 teardown and every service deploys to Cloud Run, so these \
         manifests describe infrastructure that does not exist. Delete them, \
         or update this guard if Fly deployment genuinely returns."
    );
}

#[test]
fn local_dev_database_list_matches_ci() {
    let root = workspace_root();

    // Source of truth: every `for db in ...; do` loop in the CI workflow.
    let workflow = read(&root.join(".github").join("workflows").join("rust.yml"));
    let mut workflow_sets: Vec<BTreeSet<String>> = Vec::new();
    for line in workflow.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("for db in ") {
            let list = rest.split(';').next().unwrap_or("");
            workflow_sets.push(list.split_whitespace().map(str::to_string).collect());
        }
    }
    assert!(
        !workflow_sets.is_empty(),
        "found no 'for db in ...' database loop in .github/workflows/rust.yml; \
         did the workflow change shape? Update this guard, do not delete it."
    );
    let canonical = workflow_sets[0].clone();
    for (idx, set) in workflow_sets.iter().enumerate() {
        assert_eq!(
            &canonical,
            set,
            "the database loops inside rust.yml disagree with each other \
             (occurrence 1 vs occurrence {})",
            idx + 1
        );
    }

    // scripts/postgres-init: the compose db bootstrap must create exactly
    // the CI list.
    let init_sql = read(
        &root
            .join("scripts")
            .join("postgres-init")
            .join("01-create-databases.sql"),
    );
    let sql_set: BTreeSet<String> = init_sql
        .lines()
        .filter_map(|line| line.trim().strip_prefix("CREATE DATABASE "))
        .map(|rest| rest.trim_end_matches(';').trim().to_string())
        .collect();
    assert_eq!(
        canonical, sql_set,
        "scripts/postgres-init/01-create-databases.sql disagrees with the CI \
         database list in rust.yml; edit both together"
    );

    // scripts/run-checks.ps1: crate -> database table, lines shaped
    //   "accounts-service" = "accounts"
    let ps1 = read(&root.join("scripts").join("run-checks.ps1"));
    let mut ps1_set = BTreeSet::new();
    for line in ps1.lines() {
        let parts: Vec<&str> = line.trim().split('"').collect();
        if parts.len() >= 5 && parts[1].ends_with("-service") && parts[2].trim() == "=" {
            ps1_set.insert(parts[3].to_string());
        }
    }
    assert_eq!(
        canonical, ps1_set,
        "scripts/run-checks.ps1's crate->database table disagrees with the CI \
         database list in rust.yml; edit both together"
    );

    // scripts/run-checks-wsl.sh: entries shaped "accounts-service:accounts"
    let wsl = read(&root.join("scripts").join("run-checks-wsl.sh"));
    let mut wsl_set = BTreeSet::new();
    for line in wsl.lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            if let Some((crate_name, db)) = trimmed[1..trimmed.len() - 1].split_once(':') {
                if crate_name.ends_with("-service") {
                    wsl_set.insert(db.to_string());
                }
            }
        }
    }
    assert_eq!(
        canonical, wsl_set,
        "scripts/run-checks-wsl.sh's service list disagrees with the CI \
         database list in rust.yml; edit both together"
    );

    // docker-compose.yml: every literal postgres URL must target a database
    // CI also knows about (compose deliberately runs a subset of services,
    // so this is membership, not equality).
    let compose = read(&root.join("docker-compose.yml"));
    let compose_names = postgres_database_names(&compose);
    assert!(
        !compose_names.is_empty(),
        "found no postgres:// DATABASE_URL in docker-compose.yml; \
         did the file change shape? Update this guard, do not delete it."
    );
    let unknown: Vec<&String> = compose_names
        .iter()
        .filter(|name| !canonical.contains(*name))
        .collect();
    assert!(
        unknown.is_empty(),
        "docker-compose.yml points at database name(s) {unknown:?} that CI \
         never creates; use the names from rust.yml's database loop"
    );
}
