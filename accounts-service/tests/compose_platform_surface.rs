//! Workspace drift guard for the LOCAL-DEV PLATFORM SURFACE, the sibling of
//! `no_sqlite_config.rs` (which guards the config layer's database engine).
//!
//! COMPOSE-GAP-1 (closed 2026-08-04): docker-compose.yml built `event-stream`
//! from `standalones/event-stream-service`, a path deleted in commit 949b7e2,
//! so `docker-compose up --build` had been broken since that deletion — and
//! compose ran only 6 of the 11 workspace services, so "the local platform"
//! and "the workspace" were two silently different lists. The recorded
//! decision: compose is SELF-CONTAINED — it runs exactly the services whose
//! source lives in this repo (the Cargo.toml workspace members) plus the
//! shared db; services in sibling repos run from those repos.
//!
//! Three truths, each read from BOTH sources on every run:
//! 1. Every compose build context (and its dockerfile) exists inside this
//!    repo — no `..` escapes, no deleted directories.
//! 2. The set of compose services with build blocks equals the workspace
//!    member list in Cargo.toml, both directions.
//! 3. Each compose service's DATABASE_URL names the same database CI gives
//!    that crate in .github/workflows/rust.yml, so local dev and CI cannot
//!    drift onto different schemas.
//!
//! The guard lives in accounts-service only because a Rust test must live in
//! some crate; it deliberately scans the workspace root.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').trim_matches('\'').to_string()
}

/// One compose service: its name and the raw lines of its block.
struct ComposeService {
    name: String,
    lines: Vec<String>,
}

impl ComposeService {
    /// The value of a `key:` line anywhere in this service's block, if any.
    fn value_of(&self, key: &str) -> Option<String> {
        let prefix = format!("{key}:");
        self.lines
            .iter()
            .map(|line| line.trim_start())
            .find_map(|line| line.strip_prefix(&prefix))
            .map(unquote)
    }

    fn has_build_block(&self) -> bool {
        self.lines
            .iter()
            .any(|line| line.trim_start() == "build:" || line.trim_start().starts_with("build:"))
    }

    /// Database name from the first `postgres://.../<name>` URL in the block.
    fn database_name(&self) -> Option<String> {
        for line in &self.lines {
            if let Some(pos) = line.find("postgres://") {
                let tail = &line[pos..];
                let end = tail
                    .find(|c: char| c == '"' || c == '\'' || c.is_whitespace())
                    .unwrap_or(tail.len());
                if let Some((_, db)) = tail[..end].rsplit_once('/') {
                    if !db.is_empty() && !db.contains('$') {
                        return Some(db.to_string());
                    }
                }
            }
        }
        None
    }
}

/// Parse docker-compose.yml into its top-level services. A service starts at
/// a two-space-indented `name:` line inside the `services:` block; its block
/// runs until the next such line.
fn compose_services(compose: &str) -> Vec<ComposeService> {
    let mut services: Vec<ComposeService> = Vec::new();
    let mut in_services = false;
    for line in compose.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            in_services = line.trim_end() == "services:";
            continue;
        }
        if !in_services {
            continue;
        }
        let two_space_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-');
        if two_space_key {
            services.push(ComposeService {
                name: line.trim().trim_end_matches(':').to_string(),
                lines: Vec::new(),
            });
        } else if let Some(current) = services.last_mut() {
            current.lines.push(line.to_string());
        }
    }
    services
}

/// Workspace member directories parsed from the root Cargo.toml.
fn workspace_members(root: &Path) -> BTreeSet<String> {
    let manifest = read(&root.join("Cargo.toml"));
    let mut members = BTreeSet::new();
    let mut in_members = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("members") && trimmed.contains('[') {
            in_members = true;
            continue;
        }
        if in_members {
            if trimmed.starts_with(']') {
                break;
            }
            let entry = unquote(trimmed.trim_end_matches(','));
            if !entry.is_empty() {
                members.insert(entry);
            }
        }
    }
    assert!(
        members.len() >= 11,
        "parsed only {} workspace members from Cargo.toml; did the manifest \
         change shape? Update this guard, do not delete it.",
        members.len()
    );
    members
}

/// Crate -> database mapping parsed from every
/// `DATABASE_URL=postgres://...localhost:5432/<db> cargo <cmd> -p <crate>`
/// line in rust.yml. Disagreeing duplicate entries fail the parse.
fn ci_crate_databases(root: &Path) -> BTreeMap<String, String> {
    let workflow = read(&root.join(".github").join("workflows").join("rust.yml"));
    let mut mapping: BTreeMap<String, String> = BTreeMap::new();
    for line in workflow.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("DATABASE_URL=postgres://") || !trimmed.contains(" -p ") {
            continue;
        }
        let url = trimmed.split_whitespace().next().unwrap_or("");
        let db = url.rsplit_once('/').map(|(_, db)| db.to_string());
        let crate_name = trimmed
            .split(" -p ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(str::to_string);
        if let (Some(db), Some(crate_name)) = (db, crate_name) {
            if let Some(previous) = mapping.get(&crate_name) {
                assert_eq!(
                    previous, &db,
                    "rust.yml maps crate {crate_name} to two different \
                     databases ({previous} vs {db}); fix the workflow first"
                );
            }
            mapping.insert(crate_name, db);
        }
    }
    assert!(
        mapping.len() >= 11,
        "parsed only {} crate->database lines from rust.yml; did the \
         workflow change shape? Update this guard, do not delete it.",
        mapping.len()
    );
    mapping
}

#[test]
fn compose_build_contexts_exist_inside_this_repo() {
    let root = workspace_root();
    let compose = read(&root.join("docker-compose.yml"));
    let services = compose_services(&compose);

    let mut build_services = 0usize;
    for service in &services {
        if !service.has_build_block() {
            continue;
        }
        build_services += 1;
        let context = service.value_of("context").unwrap_or_else(|| {
            panic!(
                "docker-compose.yml service '{}' has a build block with no \
                 context line this guard can read",
                service.name
            )
        });
        assert!(
            !context.contains(".."),
            "docker-compose.yml service '{}' builds from '{}', which escapes \
             this repository — compose is self-contained by decision \
             (COMPOSE-GAP-1, 2026-08-04): services whose source lives in a \
             sibling repo run from that repo, not from here",
            service.name,
            context
        );
        let context_dir = root.join(&context);
        assert!(
            context_dir.is_dir(),
            "docker-compose.yml service '{}' builds from '{}', which does not \
             exist in this repository, so `docker-compose up --build` cannot \
             succeed — the exact COMPOSE-GAP-1 defect shape",
            service.name,
            context
        );
        let dockerfile = service
            .value_of("dockerfile")
            .unwrap_or_else(|| "Dockerfile".to_string());
        let dockerfile_path = context_dir.join(&dockerfile);
        assert!(
            dockerfile_path.is_file(),
            "docker-compose.yml service '{}' names dockerfile '{}', which \
             does not exist under its build context '{}'",
            service.name,
            dockerfile,
            context
        );
    }

    assert!(
        build_services >= 11,
        "expected at least 11 compose services with build blocks, parsed {build_services}; \
         did docker-compose.yml change shape? Update this guard, do not delete it."
    );
}

#[test]
fn compose_carries_every_workspace_service() {
    let root = workspace_root();
    let members = workspace_members(&root);
    let compose = read(&root.join("docker-compose.yml"));

    let mut compose_crates = BTreeSet::new();
    for service in compose_services(&compose) {
        if !service.has_build_block() {
            continue;
        }
        let dockerfile = service
            .value_of("dockerfile")
            .unwrap_or_else(|| "Dockerfile".to_string());
        if let Some((crate_dir, _)) = dockerfile.rsplit_once('/') {
            compose_crates.insert(crate_dir.to_string());
        }
    }

    let missing: Vec<&String> = members.difference(&compose_crates).collect();
    assert!(
        missing.is_empty(),
        "workspace member(s) {missing:?} are not in docker-compose.yml — \
         compose carries ALL workspace services by decision (COMPOSE-GAP-1, \
         2026-08-04); add the service or record a new decision in the guard"
    );
    let extra: Vec<&String> = compose_crates.difference(&members).collect();
    assert!(
        extra.is_empty(),
        "docker-compose.yml builds {extra:?}, which are not workspace members \
         in Cargo.toml — compose is self-contained by decision (COMPOSE-GAP-1, \
         2026-08-04); out-of-repo services run from their own repos"
    );
}

#[test]
fn compose_database_urls_match_ci_per_crate() {
    let root = workspace_root();
    let ci = ci_crate_databases(&root);
    let compose = read(&root.join("docker-compose.yml"));

    let mut checked = 0usize;
    for service in compose_services(&compose) {
        if !service.has_build_block() {
            continue;
        }
        let dockerfile = service
            .value_of("dockerfile")
            .unwrap_or_else(|| "Dockerfile".to_string());
        let Some((crate_dir, _)) = dockerfile.rsplit_once('/') else {
            continue;
        };
        let compose_db = service.database_name().unwrap_or_else(|| {
            panic!(
                "docker-compose.yml service '{}' ({crate_dir}) has no \
                 postgres:// DATABASE_URL this guard can read",
                service.name
            )
        });
        let ci_db = ci.get(crate_dir).unwrap_or_else(|| {
            panic!(
                "rust.yml has no DATABASE_URL line for crate {crate_dir}; \
                 edit the workflow and compose together"
            )
        });
        assert_eq!(
            ci_db, &compose_db,
            "docker-compose.yml gives '{}' ({crate_dir}) database '{compose_db}' \
             but CI runs that crate against '{ci_db}' — local dev and CI would \
             test two different schemas; use CI's name",
            service.name
        );
        checked += 1;
    }

    assert!(
        checked >= 11,
        "cross-checked only {checked} compose services against rust.yml; \
         did the file change shape? Update this guard, do not delete it."
    );
}
