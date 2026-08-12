//! Behaviour coverage for the audit-event emitter's deploy wiring (v1.18 PR 2a).
//!
//! `activities-service/openapi.yaml:197` documents, in the present tense, that a
//! create "emits an audit event" fire-and-forget. That sentence was FALSE
//! in production until 2026-08-12: `emit_audit` returns at its first
//! `env::var("AUDIT_SERVICE_URL")` miss, the deploy matrix never set the
//! variable, and the caller ignores the function's `()` return — so the whole
//! audit trail was inert with every check, every deploy and every `/health`
//! probe green. Nothing in the repo could have gone red, because there was
//! nothing anywhere that observed the call.
//!
//! These tests drive the REAL `emit_audit` against a stub bound on 127.0.0.1
//! and pin both directions the deploy edit turns on:
//!   * configured -> a POST arrives at `/api/v1/audit-events` carrying the
//!     CALLER's own Authorization header and the exact body audit-service's
//!     `CreateAuditEventRequest` deserializes;
//!   * unset or blank -> no connection is made at all.
//!
//! The probe defect is one only this layer can produce (L-081): a TCP
//! connection arriving on a port nothing else in this process talks to. No
//! database is involved — `emit_audit` is a free function over a
//! `reqwest::Client`, so the suite runs in milliseconds.
//!
//! The unset case is not vacuous, and it is worth saying why: it reuses the
//! SAME listener address the configured case proved reachable, so it excludes a
//! read hoisted to start-up (a `OnceLock`/`lazy_static` URL would still connect
//! after the variable is removed) rather than merely observing that an unknown
//! address cannot be dialled.
//!
//! One clause per test case (L-072): gutting the send reddens the three
//! configured cases independently instead of aborting at the first.

use std::env;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use activities_service::handlers::activities::emit_audit;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The variable this increment added to `rust.yml`'s deploy matrix.
const VAR: &str = "AUDIT_SERVICE_URL";

/// Fixed emission the configured cases assert on. `entity_type` and `action`
/// are the literals this service's own handlers pass (`activities.rs` call sites
/// for create/update/delete), and both are members of audit-service's
/// `VALID_ENTITY_TYPES` / `VALID_ACTIONS` — see
/// `accounts-service/tests/audit_vocabulary.rs`, which asserts that platform
/// wide rather than restating it here.
const ENTITY_TYPE: &str = "activity";
const ACTION: &str = "created";
const ENTITY_ID: &str = "actv-11111111-2222-3333-4444-555555555555";
const ACTOR_ID: &str = "user-a1b2c3";
const ENTITY_LABEL: &str = "Q3 renewal call with Globex";
const AUTH_HEADER: &str = "Bearer header.payload.signature-activities";

/// `std::env` is process-global while test cases in one binary run on several
/// threads, so every case touching `AUDIT_SERVICE_URL` serialises on this.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Captured {
    request_line: String,
    headers: Vec<(String, String)>,
    body: serde_json::Value,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Accepts exactly one request, reads it whole, answers 201 and returns what
/// arrived. Everything asserted downstream is a byte the emitter actually put
/// on the wire, never a token read out of its source.
async fn capture_one(listener: TcpListener) -> Captured {
    let (mut socket, _) = listener
        .accept()
        .await
        .expect("the stub could not accept a connection");

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];

    let (head_end, content_length) = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
            let length = head
                .lines()
                .skip(1)
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.trim().eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            break (pos + 4, length);
        }
        let read = socket
            .read(&mut chunk)
            .await
            .expect("the stub failed to read the request head");
        assert!(
            read > 0,
            "the emitter closed the connection before sending a complete request head"
        );
        buf.extend_from_slice(&chunk[..read]);
    };

    while buf.len() < head_end + content_length {
        let read = socket
            .read(&mut chunk)
            .await
            .expect("the stub failed to read the request body");
        assert!(
            read > 0,
            "the emitter closed the connection with {} of {content_length} body bytes sent",
            buf.len() - head_end
        );
        buf.extend_from_slice(&chunk[..read]);
    }

    socket
        .write_all(b"HTTP/1.1 201 Created\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
        .await
        .expect("the stub failed to answer");
    let _ = socket.flush().await;

    let head = String::from_utf8_lossy(&buf[..head_end.saturating_sub(4)]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();

    let raw_body = &buf[head_end..head_end + content_length];
    let body = serde_json::from_slice(raw_body).unwrap_or_else(|err| {
        panic!(
            "the emitter sent a body audit-service could not deserialize: {err}; raw = {}",
            String::from_utf8_lossy(raw_body)
        )
    });

    Captured {
        request_line,
        headers,
        body,
    }
}

/// Binds a stub, points `AUDIT_SERVICE_URL` at it, runs the real emitter once
/// and returns what the stub received. The caller must already hold `env_lock`.
///
/// The configured URL carries a TRAILING SLASH on purpose: `emit_audit` builds
/// its path with `trim_end_matches('/')`, so a base URL written either way must
/// produce the same single-slash route.
async fn emit_against_stub() -> Captured {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");
    let addr: SocketAddr = listener.local_addr().expect("no local addr");
    env::set_var(VAR, format!("http://{addr}/"));

    let server = tokio::spawn(capture_one(listener));
    let client = reqwest::Client::new();
    emit_audit(
        &client,
        ENTITY_TYPE,
        ENTITY_ID,
        ACTION,
        ACTOR_ID,
        Some(ENTITY_LABEL),
        AUTH_HEADER,
    )
    .await;
    env::remove_var(VAR);

    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("the emitter sent nothing to the configured audit service within 10s")
        .expect("the stub task panicked")
}

/// Binds a stub, leaves `AUDIT_SERVICE_URL` in the state the caller set, runs
/// the real emitter and reports whether anything connected. The caller must
/// already hold `env_lock`.
async fn nothing_connects_within(grace: Duration) -> bool {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");

    let client = reqwest::Client::new();
    emit_audit(
        &client,
        ENTITY_TYPE,
        ENTITY_ID,
        ACTION,
        ACTOR_ID,
        Some(ENTITY_LABEL),
        AUTH_HEADER,
    )
    .await;

    tokio::time::timeout(grace, listener.accept()).await.is_err()
}

#[tokio::test]
async fn posts_to_the_audit_events_route_of_the_configured_base_url() {
    let _guard = env_lock();
    let captured = emit_against_stub().await;

    assert_eq!(
        captured.request_line, "POST /api/v1/audit-events HTTP/1.1",
        "the emitter must POST audit-service's documented ingest route, built \
         from AUDIT_SERVICE_URL with exactly one slash regardless of whether the \
         deploy value ends in one"
    );
}

#[tokio::test]
async fn forwards_the_callers_authorization_header_verbatim() {
    let _guard = env_lock();
    let captured = emit_against_stub().await;

    assert_eq!(
        captured.header("authorization"),
        Some(AUTH_HEADER),
        "audit-service's POST requires the admin or service role, and this path \
         mints no credential of its own — it forwards the caller's token \
         unchanged. If this ever stops holding, every emitted event becomes a \
         silent 401/403 that the fire-and-forget caller discards."
    );
}

#[tokio::test]
async fn sends_the_exact_body_audit_service_deserializes() {
    let _guard = env_lock();
    let captured = emit_against_stub().await;

    assert_eq!(
        captured.header("content-type"),
        Some("application/json"),
        "audit-service's ingest takes `Json(CreateAuditEventRequest)`, which \
         rejects any other content type before the handler runs"
    );
    assert_eq!(
        captured.body,
        serde_json::json!({
            "entity_type": ENTITY_TYPE,
            "entity_id": ENTITY_ID,
            "action": ACTION,
            "actor_id": ACTOR_ID,
            "entity_label": ENTITY_LABEL,
        }),
        "the body must match audit-service's CreateAuditEventRequest field for \
         field: equality (not containment) is deliberate, so a field silently \
         added or renamed here is caught rather than tolerated"
    );
}

#[tokio::test]
async fn emits_nothing_when_the_audit_service_url_is_unset() {
    let _guard = env_lock();
    env::remove_var(VAR);

    assert!(
        nothing_connects_within(Duration::from_millis(750)).await,
        "with AUDIT_SERVICE_URL unset the emitter must make no request at all; a \
         connection here means the URL was captured once at start-up rather than \
         read per call, which would make the deploy variable unremovable"
    );
}

#[tokio::test]
async fn emits_nothing_when_the_audit_service_url_is_blank() {
    let _guard = env_lock();
    env::set_var(VAR, "   ");
    let quiet = nothing_connects_within(Duration::from_millis(750)).await;
    env::remove_var(VAR);

    assert!(
        quiet,
        "a matrix entry set to an empty value takes the `url.trim().is_empty()` \
         branch, which is a different branch from the `env::var` miss above and \
         must also make no request"
    );
}
