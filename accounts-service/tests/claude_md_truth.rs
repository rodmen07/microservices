//! Drift guard between CLAUDE.md (the agent-facing instruction file) and the
//! workspace it describes. Sibling of `no_sqlite_config.rs` (config layer) and
//! `compose_platform_surface.rs` (compose layer); this one covers the PROSE
//! layer, because prose asserting a false state is this repo's most durable
//! defect class: CLAUDE.md claimed a `SqlitePool` architecture and a
//! `d:\Projects\microservices\frontend-service` frontend for months after both
//! stopped being true (fixed 2026-08-08), and nothing could go red over it.
//!
//! Two agreements are held, each read from BOTH sources on every run:
//!
//! 1. The service inventory: every `[workspace] members` crate in the root
//!    Cargo.toml is named in CLAUDE.md, so adding or renaming a service
//!    without documenting it reddens CI instead of drifting silently.
//! 2. The persistence claim: every service's `app_state.rs` names `PgPool`,
//!    and CLAUDE.md never names `SqlitePool`. Together these pin the doc and
//!    the code to the same answer about what the pool type is.
//!
//! Plus one tombstone ban: the pre-Portfolio repo path (`Projects\microservices`
//! directly under Projects) must not reappear; the repo lives under
//! `d:\Projects\Portfolio\microservices` since the Portfolio superproject split.
//!
//! Scope honesty: CLAUDE.md matches rust.yml's DOCS_RE, so a docs-only PR that
//! edits CLAUDE.md skips the cargo jobs and this guard runs on the NEXT
//! code-classified run rather than on that PR itself. The drift is caught late,
//! not never. The guard lives in accounts-service only because a Rust test must
//! live in some crate; it deliberately scans the workspace root.

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

/// Crate names from the root Cargo.toml `[workspace] members` list.
fn workspace_members(root: &Path) -> Vec<String> {
    let manifest = read(&root.join("Cargo.toml"));
    let mut members = Vec::new();
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
            if let Some(name) = trimmed.trim_end_matches(',').trim().strip_prefix('"') {
                if let Some(name) = name.strip_suffix('"') {
                    members.push(name.to_string());
                }
            }
        }
    }
    assert!(
        members.len() >= 11,
        "parsed only {} workspace members from the root Cargo.toml; the \
         workspace has 11 services, so the members parse went blind. Update \
         this guard, do not delete it.",
        members.len()
    );
    members
}

#[test]
fn workspace_members_are_all_named_in_claude_md() {
    let root = workspace_root();
    let claude_md = read(&root.join("CLAUDE.md"));
    let missing: Vec<String> = workspace_members(&root)
        .into_iter()
        .filter(|name| !claude_md.contains(name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "CLAUDE.md's service inventory is missing workspace member(s) \
         {missing:?}; its 'What this project is' section must name every \
         [workspace] members crate, or an agent reading it plans against a \
         wrong platform surface"
    );
}

#[test]
fn every_service_app_state_uses_pgpool() {
    let root = workspace_root();
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for name in workspace_members(&root) {
        let app_state = root
            .join(&name)
            .join("src")
            .join("lib")
            .join("app_state.rs");
        let source = read(&app_state);
        scanned += 1;
        if !source.contains("PgPool") {
            offenders.push(name);
        }
    }
    assert!(
        scanned >= 11,
        "scanned only {scanned} app_state.rs files; the corpus went blind. \
         Update this guard, do not delete it."
    );
    assert!(
        offenders.is_empty(),
        "service(s) {offenders:?} no longer name PgPool in app_state.rs, but \
         CLAUDE.md documents a PgPool-everywhere architecture; change the \
         doc and this guard together, deliberately"
    );
}

#[test]
fn claude_md_does_not_claim_sqlitepool() {
    let root = workspace_root();
    let claude_md = read(&root.join("CLAUDE.md"));
    let offenders: Vec<String> = claude_md
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("SqlitePool"))
        .map(|(idx, line)| format!("CLAUDE.md:{}: {}", idx + 1, line.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "CLAUDE.md claims a SqlitePool architecture again; every service has \
         been PgPool since v1.5.0 and no .rs file names SqlitePool. This exact \
         claim previously sat false at CLAUDE.md:49 for months:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn claude_md_does_not_use_the_pre_portfolio_repo_path() {
    let root = workspace_root();
    let claude_md = read(&root.join("CLAUDE.md"));
    let offenders: Vec<String> = claude_md
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.contains(r"Projects\microservices") || line.contains("Projects/microservices")
        })
        .map(|(idx, line)| format!("CLAUDE.md:{}: {}", idx + 1, line.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "CLAUDE.md names the pre-Portfolio repo path again; this repo lives at \
         d:\\Projects\\Portfolio\\microservices (and the frontend at \
         d:\\Projects\\Portfolio\\infraportal), so a bare Projects\\microservices \
         path sends an agent to a directory that does not exist:\n{}",
        offenders.join("\n")
    );
}
