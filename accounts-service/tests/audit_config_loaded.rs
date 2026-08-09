//! Workspace drift guard for the cargo-audit configuration — the regression
//! proof for AUDIT-CONFIG-DEAD-1.
//!
//! The bug: the accepted-advisory list lived at the repo root as `audit.toml`,
//! a location cargo-audit NEVER loads (it reads `.cargo/audit.toml` relative
//! to the directory it runs from). The gate behaved anyway because both
//! workflows duplicated the ignore as a CLI `--ignore` flag, so the dead file
//! was invisible: a policy file the tool never found is indistinguishable from
//! a policy that passed. The next accepted advisory recorded there would have
//! been silently unenforced.
//!
//! The fix moved the file to `.cargo/audit.toml` and dropped the CLI flags, so
//! the config is the ONE enforcing home. This guard pins the silent
//! directions, the ones no gate reddens on its own:
//!
//! 1. no config file at the dead root location (it would be ignored, and it
//!    would teach readers to record decisions where they do not count);
//! 2. the loaded location exists and declares an `[advisories]` section;
//! 3. the two workflows' audit commands are byte-identical (previously a
//!    hand-run check on PR #125, now enforced);
//! 4. no workflow audit command carries a CLI `--ignore` (a second enforcing
//!    home that can drift from the config is exactly how this bug survived).
//!
//! Deliberately NOT asserted: the CONTENT of the ignore list. Dropping an
//! entry is loud on its own (the required `Security audit` context goes red
//! on the advisory), and pinning content here would make a legitimate future
//! un-ignore red the wrong test.
//!
//! The guard lives in accounts-service only because a Rust test must live in
//! some crate; it deliberately reads workspace-root files.

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf()
}

/// The audit run-commands (one per workflow), extracted from `run:` lines.
/// Comment lines mentioning `cargo audit` are not commands and are skipped.
fn workflow_audit_commands() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut commands = Vec::new();
    for name in ["rust.yml", "security-audit.yml"] {
        let path = root.join(".github").join("workflows").join(name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        for line in source.lines() {
            let trimmed = line.trim_start();
            if let Some(cmd) = trimmed.strip_prefix("run: ") {
                if cmd.contains("cargo audit") {
                    commands.push((name.to_string(), cmd.trim().to_string()));
                }
            }
        }
    }
    // Negative control: both workflows must still contain an audit command,
    // or this guard is reading air. Update the guard, do not delete it.
    assert!(
        commands.len() >= 2,
        "expected a `run: cargo audit ...` line in both rust.yml and \
         security-audit.yml, found {}: {commands:?}; did a workflow change \
         shape or lose its audit step?",
        commands.len()
    );
    commands
}

#[test]
fn no_audit_config_at_the_dead_root_location() {
    let root = workspace_root();
    let dead = root.join("audit.toml");
    assert!(
        !dead.exists(),
        "{} exists, but cargo-audit NEVER loads a root audit.toml — it reads \
         .cargo/audit.toml relative to where it runs. A file here is dead \
         config: any accepted advisory recorded in it is silently unenforced \
         (AUDIT-CONFIG-DEAD-1). Record the decision in .cargo/audit.toml and \
         delete this file.",
        dead.display()
    );
}

#[test]
fn loaded_audit_config_exists_and_declares_advisories() {
    let root = workspace_root();
    let config = root.join(".cargo").join("audit.toml");
    let source = fs::read_to_string(&config).unwrap_or_else(|err| {
        panic!(
            "{}: {err} — this file is the ONE enforcing home of accepted \
             advisories since AUDIT-CONFIG-DEAD-1 was fixed; without it the \
             required `Security audit` context reds on every accepted \
             advisory (currently RUSTSEC-2023-0071, which has no upstream fix)",
            config.display()
        )
    });
    assert!(
        source.contains("[advisories]"),
        "{} exists but declares no [advisories] section, so cargo-audit \
         loads nothing from it; the accepted-advisory list must live there",
        config.display()
    );
}

#[test]
fn both_workflows_run_the_identical_audit_command() {
    let commands = workflow_audit_commands();
    let (first_file, first_cmd) = &commands[0];
    for (file, cmd) in &commands[1..] {
        assert_eq!(
            first_cmd, cmd,
            "the audit commands in {first_file} and {file} differ; they must \
             stay byte-identical so the required PR gate and the scheduled \
             audit cannot drift apart (previously a hand-run check on PR #125, \
             enforced here since AUDIT-CONFIG-DEAD-1)"
        );
    }
}

#[test]
fn accepted_advisories_have_exactly_one_enforcing_home() {
    let offenders: Vec<String> = workflow_audit_commands()
        .into_iter()
        .filter(|(_, cmd)| cmd.contains("--ignore"))
        .map(|(file, cmd)| format!("{file}: {cmd}"))
        .collect();
    assert!(
        offenders.is_empty(),
        "workflow audit command(s) carry a CLI --ignore:\n{}\n\
         The accepted-advisory list lives in .cargo/audit.toml ALONE. A CLI \
         duplicate is a second enforcing home, and a second home that can \
         drift from the first is exactly how AUDIT-CONFIG-DEAD-1 survived: \
         the config sat unloaded for months while the flag did the work.",
        offenders.join("\n")
    );
}
