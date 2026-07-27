//! Workspace drift guard: no service binary may reference sqlite.
//!
//! Every workspace service compiles sqlx with postgres-only features
//! (`runtime-tokio-rustls`, `postgres`, ...), so a `sqlite://` DATABASE_URL
//! fallback can never connect: it turns "DATABASE_URL is not set" into a
//! misleading driver error at boot, and the old "falling back to in-memory
//! sqlite" arm would silently serve an empty database if the sqlite driver
//! were ever re-enabled. Those fallbacks were removed in 2026-07; this guard
//! keeps the class from returning in any service entrypoint.
//!
//! The guard lives in accounts-service only because a Rust test must live in
//! some crate; it deliberately scans every sibling `*-service/src/main.rs`
//! from the workspace root so a regression in any service reddens CI.

use std::fs;
use std::path::PathBuf;

#[test]
fn no_service_entrypoint_references_sqlite() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf();

    let mut scanned = 0usize;
    let mut offenders = Vec::new();

    for entry in fs::read_dir(&workspace_root).expect("read workspace root") {
        let entry = entry.expect("read workspace dir entry");
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        if !dir_name.ends_with("-service") {
            continue;
        }
        let main_rs = entry.path().join("src").join("main.rs");
        if !main_rs.is_file() {
            continue;
        }
        let source = fs::read_to_string(&main_rs)
            .unwrap_or_else(|err| panic!("read {dir_name}/src/main.rs: {err}"));
        scanned += 1;
        for (idx, line) in source.lines().enumerate() {
            if line.to_ascii_lowercase().contains("sqlite") {
                offenders.push(format!(
                    "{dir_name}/src/main.rs:{}: {}",
                    idx + 1,
                    line.trim()
                ));
            }
        }
    }

    // Negative control: the scan must actually have seen the workspace's
    // service entrypoints, otherwise a layout change could make this test
    // pass vacuously while scanning nothing.
    assert!(
        scanned >= 11,
        "expected to scan at least 11 service entrypoints, scanned {scanned}; \
         did the workspace layout change? Update this guard, do not delete it."
    );

    assert!(
        offenders.is_empty(),
        "sqlite reference(s) in postgres-only service entrypoints \
         (dead fallback code; require or default DATABASE_URL to postgres instead):\n{}",
        offenders.join("\n")
    );
}
