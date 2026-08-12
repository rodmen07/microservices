//! Behaviour coverage for the three-state peer existence check
//! (v1.18 PR 2b slice 1, closing PEER-CHECK-FAILS-CLOSED-1).
//!
//! Before this increment `account_exists` / `contact_exists` returned `bool`,
//! so four observable situations collapsed into two:
//!
//! | the peer ...            | old result | old client-visible response |
//! |-------------------------|------------|-----------------------------|
//! | answers 200             | `true`     | the request proceeds        |
//! | answers 404             | `false`    | 422 INVALID_ACCOUNT         |
//! | answers 403             | `false`    | 422 INVALID_ACCOUNT         |
//! | refuses the connection  | `false`    | 422 INVALID_ACCOUNT         |
//!
//! The bottom two rows are a platform fault reported to the client as bad
//! input. These tests pin all four apart by driving the REAL `check_peer`
//! against a stub bound on `127.0.0.1`, and then pin the status each verdict
//! turns into.
//!
//! The perturbation each case is proof against is different, which is why they
//! are separate `#[tokio::test]`s rather than one table (L-072): folding
//! `Rejected` back into `Absent` reddens exactly one case here, and folding
//! `Unreachable` back into `Absent` reddens exactly one other. A single
//! assertion over a vector would have gone red once for either and named
//! neither.
//!
//! `check_peer` takes the base URL as a parameter rather than reading
//! `env::var` itself, so no case here touches process-global state and they run
//! in parallel safely. That signature is not a testing convenience: the first
//! draft did read the variable inside the function, and
//! `accounts-service/tests/deploy_env_surface.rs` refused to report a verdict
//! on the whole workspace because the `env::var` argument was no longer a
//! literal it could attribute to a service. What THIS suite therefore does not
//! cover is the literal read at each call site; `deploy_env_surface` covers
//! exactly that, and covers it for all eleven services at once.
//!
//! No database is involved — `check_peer` is a free function over a
//! `reqwest::Client` — so the suite runs in milliseconds.

use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::time::Duration;

use activities_service::peer_check::{self, check_peer, PeerCheck};
use axum::http::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PATH: &str = "api/v1/accounts";
const ID: &str = "acct-11111111-2222-3333-4444-555555555555";
const AUTH_HEADER: &str = "Bearer header.payload.signature-activities";

/// Reads one request whole, answers with `status_line`, and returns the request
/// line so a case can prove the checker asked the route it claims to ask.
async fn serve_one(listener: TcpListener, status_line: &'static str) -> String {
    let (mut socket, _) = listener
        .accept()
        .await
        .expect("the stub could not accept a connection");

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        let read = socket
            .read(&mut chunk)
            .await
            .expect("the stub failed to read the request");
        assert!(read > 0, "the checker closed the connection mid-request");
        buf.extend_from_slice(&chunk[..read]);
    }

    let response =
        format!("HTTP/1.1 {status_line}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
    socket
        .write_all(response.as_bytes())
        .await
        .expect("the stub failed to answer");
    let _ = socket.flush().await;

    let head = String::from_utf8_lossy(&buf).into_owned();
    head.lines().next().unwrap_or_default().to_string()
}

/// Points the checker at a stub answering `status_line`, runs it once, and
/// returns its verdict plus what the stub received.
///
/// The base URL carries a TRAILING SLASH on purpose: `check_peer` builds its
/// path with `trim_end_matches('/')`, so a deploy value written either way must
/// produce the same single-slash route.
async fn check_against(status_line: &'static str) -> (PeerCheck, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");
    let addr: SocketAddr = listener.local_addr().expect("no local addr");
    let base_url = format!("http://{addr}/");

    let server = tokio::spawn(serve_one(listener, status_line));
    let client = reqwest::Client::new();
    let verdict = check_peer(&client, Some(&base_url), PATH, ID, AUTH_HEADER).await;

    let request_line = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("the checker sent nothing to the configured peer within 10s")
        .expect("the stub task panicked");

    (verdict, request_line)
}

/// A `127.0.0.1` port with nothing listening on it.
///
/// Bound and immediately dropped rather than guessed, so the address is
/// provably free at this instant instead of merely unlikely to be taken.
fn closed_port() -> SocketAddr {
    let probe = StdTcpListener::bind("127.0.0.1:0").expect("could not bind a probe socket");
    let addr = probe.local_addr().expect("no local addr");
    drop(probe);
    addr
}

/// Runs the checker with `base_url` and reports whether anything connected to a
/// listener bound for the occasion.
///
/// Binding a real listener and proving nothing arrives is stronger than
/// observing that an unknown address is not dialled: it excludes a checker that
/// dials anyway and fails, which would also produce "no verdict of Exists".
async fn nothing_connects_for(base_url: Option<&str>) -> (PeerCheck, bool) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");

    let client = reqwest::Client::new();
    let verdict = check_peer(&client, base_url, PATH, ID, AUTH_HEADER).await;

    let quiet = tokio::time::timeout(Duration::from_millis(750), listener.accept())
        .await
        .is_err();
    (verdict, quiet)
}

/// The body a `Response` carries, read back off the wire.
async fn body_of(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("could not read the error body");
    serde_json::from_slice(&bytes).expect("the error body was not JSON")
}

// ---------------------------------------------------------------------------
// What the checker observes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_two_hundred_from_the_peer_is_exists() {
    let (verdict, request_line) = check_against("200 OK").await;

    assert_eq!(verdict, PeerCheck::Exists);
    assert_eq!(
        request_line,
        format!("GET /{PATH}/{ID} HTTP/1.1"),
        "the checker must ask the peer's documented read route, built from the \
         configured base URL with exactly one slash regardless of whether the \
         deploy value ends in one"
    );
}

#[tokio::test]
async fn a_four_oh_four_from_the_peer_is_absent() {
    let (verdict, _) = check_against("404 Not Found").await;

    assert_eq!(
        verdict,
        PeerCheck::Absent,
        "404 is the ONLY answer that means the caller's id is wrong, and the \
         only one that may keep the documented 422"
    );
}

#[tokio::test]
async fn a_four_oh_three_from_the_peer_is_rejected_not_absent() {
    let (verdict, _) = check_against("403 Forbidden").await;

    assert_eq!(
        verdict,
        PeerCheck::Rejected(403),
        "a 403 is this service's forwarded token being refused by the peer's own \
         role gate — a platform misconfiguration. Collapsing it into Absent is \
         the defect PEER-CHECK-FAILS-CLOSED-1 records, and it tells the client \
         their perfectly valid account_id does not exist."
    );
}

#[tokio::test]
async fn a_refused_connection_is_unreachable_not_absent() {
    let addr = closed_port();
    let base_url = format!("http://{addr}");

    let client = reqwest::Client::new();
    let verdict = check_peer(&client, Some(&base_url), PATH, ID, AUTH_HEADER).await;

    assert_eq!(
        verdict,
        PeerCheck::Unreachable,
        "a transport failure (connect refused, DNS, TLS, or the client timeout \
         expiring on a cold Cloud Run instance) says nothing whatever about \
         whether the record exists"
    );
}

#[tokio::test]
async fn no_configured_url_is_not_configured_and_contacts_nobody() {
    let (verdict, quiet) = nothing_connects_for(None).await;

    assert_eq!(
        verdict,
        PeerCheck::NotConfigured,
        "with no peer URL the check is skipped, which is the historic fail-open \
         behaviour every deployed revision is running today"
    );
    assert!(
        quiet,
        "an unconfigured peer must produce no connection at all"
    );
}

#[tokio::test]
async fn a_blank_url_is_not_configured_and_contacts_nobody() {
    let (verdict, quiet) = nothing_connects_for(Some("   ")).await;

    assert_eq!(
        verdict,
        PeerCheck::NotConfigured,
        "a matrix entry present but empty is a different branch from the absent \
         one above; without it the formatted URL is `http:///api/v1/...`, which \
         fails in transport and would surface as a 503 blaming a peer that was \
         never named"
    );
    assert!(
        quiet,
        "a blank peer URL must produce no connection either — reaching transport \
         at all is the defect this branch exists to prevent"
    );
}

// ---------------------------------------------------------------------------
// What each verdict turns into for the client
// ---------------------------------------------------------------------------

#[tokio::test]
async fn only_exists_and_not_configured_let_a_request_proceed() {
    assert!(PeerCheck::Exists.passes());
    assert!(PeerCheck::NotConfigured.passes());
    assert!(!PeerCheck::Absent.passes());
    assert!(!PeerCheck::Unreachable.passes());
    assert!(!PeerCheck::Rejected(403).passes());
}

#[tokio::test]
async fn unreachable_answers_503_upstream_unavailable() {
    let response = peer_check::unreachable("account_id", "accounts-service");
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a peer that never answered is retryable and the client's input may be \
         perfectly valid — the exact thing the old 422 denied"
    );
    assert_eq!(
        body_of(response).await,
        serde_json::json!({
            "code": "UPSTREAM_UNAVAILABLE",
            "message": "accounts-service could not be reached to validate account_id",
            "details": { "field": "account_id", "upstream": "accounts-service" },
        })
    );
}

#[tokio::test]
async fn rejected_answers_502_and_echoes_the_peers_status() {
    let response = peer_check::rejected("contact_id", "contacts-service", 403);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        body_of(response).await,
        serde_json::json!({
            "code": "UPSTREAM_REJECTED",
            "message": "contacts-service refused the validation request for contact_id",
            "details": {
                "field": "contact_id",
                "upstream": "contacts-service",
                "upstream_status": 403,
            },
        }),
        "the peer's own status belongs in the payload: without it an operator \
         cannot tell a refused token from a peer 500 without reading logs"
    );
}
