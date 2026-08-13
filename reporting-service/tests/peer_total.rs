//! Behaviour coverage for the five-state dashboard rollup fetch
//! (v1.18 PR 2b slice 2).
//!
//! Before this increment the dashboard's fan-out was
//! `fetch_service_total(..) -> Result<Option<i64>, Response>`, which answered
//! five situations with three shapes:
//!
//! | the peer ...                     | old result    | old client-visible response |
//! |----------------------------------|---------------|-----------------------------|
//! | no URL configured                | `Ok(None)`    | that field is `null`        |
//! | answers 200 with `{"total": n}`  | `Ok(Some(n))` | that field is `n`           |
//! | answers 200 with no `total`      | `Ok(None)`    | that field is `null`        |
//! | answers 403 / 404 / 5xx          | `Ok(None)`    | that field is `null`        |
//! | refuses the connection           | `Err(..)`     | **500 for the whole view**  |
//! | answers 200 with a non-JSON body | `Err(..)`     | **500 for the whole view**  |
//!
//! The bottom two rows discarded the local report count, the metric list and
//! the three siblings that answered, because ONE of four services was cold —
//! while the same service answering `503` cost one `null`. These tests pin all
//! six apart by driving the REAL `fetch_peer_total` against a stub bound on
//! `127.0.0.1`, then pin what each verdict contributes to the aggregate.
//!
//! Each case is its own `#[tokio::test]` rather than one table (L-072): folding
//! `Unreachable` back into an abort reddens exactly one, folding `Rejected`
//! into `Total` reddens exactly one other, and a single assertion over a vector
//! would have gone red once for either and named neither.
//!
//! `fetch_peer_total` takes the base URL as a parameter rather than reading
//! `env::var` itself, so no case here touches process-global state and they run
//! in parallel safely. That signature is not a testing convenience — see the
//! module's own doc comment, and `accounts-service/tests/deploy_env_surface.rs`,
//! which is what covers the four literal reads at the call sites for all eleven
//! services at once.
//!
//! No database is involved: `fetch_peer_total` is a free function over a
//! `reqwest::Client`.

use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::time::{Duration, Instant};

use reporting_service::peer_total::{build_peer_client, fetch_peer_total, PeerTotal, PEER_TIMEOUT};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const ENDPOINT: &str = "api/v1/accounts";
const AUTH_HEADER: &str = "Bearer header.payload.signature-reporting";

/// Reads one request whole, answers with `status_line` and `body`, and returns
/// the request line so a case can prove the fetch asked the route it claims to.
async fn serve_one(listener: TcpListener, status_line: &'static str, body: &'static str) -> String {
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
        assert!(read > 0, "the fetch closed the connection mid-request");
        buf.extend_from_slice(&chunk[..read]);
    }

    let response = format!(
        "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    socket
        .write_all(response.as_bytes())
        .await
        .expect("the stub failed to answer");
    let _ = socket.flush().await;

    let head = String::from_utf8_lossy(&buf).into_owned();
    head.lines().next().unwrap_or_default().to_string()
}

/// Points the fetch at a stub answering `status_line`/`body`, runs it once, and
/// returns its verdict plus the request line the stub received.
///
/// The base URL carries a TRAILING SLASH on purpose: the URL is built with
/// `trim_end_matches('/')`, so a deploy value written either way must produce
/// the same single-slash route.
async fn fetch_against(
    status_line: &'static str,
    body: &'static str,
    owner_id: Option<&str>,
) -> (PeerTotal, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");
    let addr: SocketAddr = listener.local_addr().expect("no local addr");
    let base_url = format!("http://{addr}/");

    let server = tokio::spawn(serve_one(listener, status_line, body));
    let client = build_peer_client();
    let verdict = fetch_peer_total(&client, &base_url, ENDPOINT, AUTH_HEADER, owner_id).await;

    let request_line = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("the fetch sent nothing to the configured peer within 10s")
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

/// Runs the fetch with `base_url` and reports whether anything connected to a
/// listener bound for the occasion.
///
/// Binding a real listener and proving nothing arrives is stronger than
/// observing that an unknown address is not dialled: it excludes a fetch that
/// dials anyway and fails, which would also produce "no verdict of Total".
async fn nothing_connects_for(base_url: &str) -> (PeerTotal, bool) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");

    let client = build_peer_client();
    let verdict = fetch_peer_total(&client, base_url, ENDPOINT, AUTH_HEADER, None).await;

    let quiet = tokio::time::timeout(Duration::from_millis(750), listener.accept())
        .await
        .is_err();
    (verdict, quiet)
}

// ---------------------------------------------------------------------------
// What the fetch observes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_two_hundred_carrying_a_total_is_that_total() {
    let (verdict, request_line) = fetch_against("200 OK", r#"{"total":42}"#, None).await;

    assert_eq!(verdict, PeerTotal::Total(42));
    assert_eq!(
        request_line,
        format!("GET /{ENDPOINT}?limit=1 HTTP/1.1"),
        "the fetch must ask the peer's documented list route with limit=1, built \
         from the configured base URL with exactly one slash regardless of \
         whether the deploy value ends in one"
    );
}

#[tokio::test]
async fn an_owner_filter_is_forwarded_as_a_query_parameter() {
    let (verdict, request_line) = fetch_against("200 OK", r#"{"total":7}"#, Some("user-abc")).await;

    assert_eq!(verdict, PeerTotal::Total(7));
    assert_eq!(
        request_line,
        format!("GET /{ENDPOINT}?limit=1&owner_id=user-abc HTTP/1.1"),
        "the user_id scoping the dashboard applies locally must reach the \
         siblings too, or an admin scoping to one user sees that user's report \
         count beside platform-wide entity totals"
    );
}

#[tokio::test]
async fn a_two_hundred_without_a_numeric_total_is_no_total() {
    let (verdict, _) = fetch_against("200 OK", r#"{"items":[]}"#, None).await;

    assert_eq!(
        verdict,
        PeerTotal::NoTotal,
        "a 2xx whose body carries no numeric total is the documented null case \
         and must not be mistaken for a peer failure"
    );
}

#[tokio::test]
async fn a_two_hundred_with_a_non_json_body_is_no_total_not_an_abort() {
    let (verdict, _) = fetch_against("200 OK", "not json at all", None).await;

    assert_eq!(
        verdict,
        PeerTotal::NoTotal,
        "this used to be `HTTP_ERROR \"invalid service response\"` — a 500 for \
         the WHOLE dashboard, discarding the local report count and the three \
         siblings that answered, because one peer sent a malformed body"
    );
}

#[tokio::test]
async fn a_four_oh_three_is_rejected_and_carries_the_status() {
    let (verdict, _) = fetch_against("403 Forbidden", "{}", None).await;

    assert_eq!(
        verdict,
        PeerTotal::Rejected(403),
        "a 403 is this service's forwarded token being refused by the peer's own \
         role gate — a platform misconfiguration. It contributes null like every \
         other failure, but an operator must be able to tell it from a peer \
         outage without reading the peer's logs, so the status is carried."
    );
}

#[tokio::test]
async fn a_five_oh_three_is_rejected_and_carries_the_status() {
    let (verdict, _) = fetch_against("503 Service Unavailable", "{}", None).await;

    assert_eq!(
        verdict,
        PeerTotal::Rejected(503),
        "the peer being down but able to say so. This is the row that makes the \
         old split arbitrary: it cost one null, while the SAME peer refusing the \
         connection cost the entire dashboard a 500."
    );
}

#[tokio::test]
async fn a_refused_connection_is_unreachable_not_an_abort() {
    let addr = closed_port();
    let base_url = format!("http://{addr}");

    let client = build_peer_client();
    let verdict = fetch_peer_total(&client, &base_url, ENDPOINT, AUTH_HEADER, None).await;

    assert_eq!(
        verdict,
        PeerTotal::Unreachable,
        "this used to be `HTTP_ERROR \"service request failed\"` — a 500 for the \
         whole view. The dashboard fans out to FOUR services and the failure is \
         disjunctive, so it aborted whenever ANY ONE of them was cold, and the \
         dashboard is the thing that wakes them."
    );
}

#[tokio::test]
async fn an_unset_url_is_not_configured_and_contacts_nobody() {
    let (verdict, quiet) = nothing_connects_for("").await;

    assert_eq!(
        verdict,
        PeerTotal::NotConfigured,
        "the state every deployed revision takes today for all four siblings, \
         because no *_SERVICE_URL is in the deploy matrix yet"
    );
    assert!(
        quiet,
        "an unconfigured peer must produce no connection at all"
    );
}

#[tokio::test]
async fn a_blank_url_is_not_configured_and_contacts_nobody() {
    let (verdict, quiet) = nothing_connects_for("   ").await;

    assert_eq!(
        verdict,
        PeerTotal::NotConfigured,
        "a matrix entry present but empty is a different branch from the absent \
         one above; without it the formatted URL is `http:///api/v1/...`, which \
         fails in transport and would be reported as an outage of a peer that \
         was never configured"
    );
    assert!(
        quiet,
        "a blank peer URL must produce no connection either — reaching transport \
         at all is the defect this branch exists to prevent"
    );
}

// ---------------------------------------------------------------------------
// The client the deployed handler actually holds
// ---------------------------------------------------------------------------

/// A peer that accepts the connection and then never answers.
///
/// This is the failure `reqwest::Client::new()` could not survive: it sets NO
/// default timeout, so the old per-call client waited forever and the dashboard
/// hung until something upstream gave up. Driving `build_peer_client()` — the
/// same constructor `AppState` calls, not a look-alike built here — is what
/// makes this a statement about the deployed client rather than about a client
/// this test configured to pass.
#[tokio::test]
async fn a_peer_that_accepts_and_never_answers_is_unreachable_within_the_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("could not bind the stub");
    let addr: SocketAddr = listener.local_addr().expect("no local addr");
    let base_url = format!("http://{addr}");

    // Hold the connection open and say nothing, for longer than the timeout.
    let _server = tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            tokio::time::sleep(PEER_TIMEOUT * 4).await;
            drop(socket);
        }
    });

    let client = build_peer_client();
    let started = Instant::now();
    let verdict = fetch_peer_total(&client, &base_url, ENDPOINT, AUTH_HEADER, None).await;
    let elapsed = started.elapsed();

    assert_eq!(
        verdict,
        PeerTotal::Unreachable,
        "a hung peer must resolve to a verdict, not hang the dashboard"
    );
    assert!(
        elapsed < PEER_TIMEOUT * 2,
        "the fetch returned after {elapsed:?}, which is not bounded by \
         PEER_TIMEOUT ({PEER_TIMEOUT:?}) — the client is being built without a \
         timeout, which is the state `reqwest::Client::new()` left the dashboard in"
    );
}

// ---------------------------------------------------------------------------
// What each verdict contributes to the aggregate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn only_total_contributes_a_number_and_nothing_aborts_the_view() {
    assert_eq!(PeerTotal::Total(9).total(), Some(9));
    assert_eq!(PeerTotal::NotConfigured.total(), None);
    assert_eq!(PeerTotal::NoTotal.total(), None);
    assert_eq!(PeerTotal::Unreachable.total(), None);
    assert_eq!(PeerTotal::Rejected(403).total(), None);
    assert_eq!(PeerTotal::Rejected(503).total(), None);
}

// ---------------------------------------------------------------------------
// Characterisation of a filed-not-fixed defect (L-052)
// ---------------------------------------------------------------------------

/// Pins TODAY'S wrong behaviour for `DASHBOARD-OWNER-UNENCODED-1`.
///
/// `owner_id` is interpolated into the peer URL with no percent-encoding, and
/// `?user_id=` is admin-controlled on this route, so an admin can inject extra
/// query parameters into an internal service call. It is latent while the four
/// `*_SERVICE_URL` variables are unset and goes live with v1.18 PR 2b's
/// remaining slice, which is why it is filed with a close condition rather than
/// folded into this diff.
///
/// **Fixing the bug MUST redden this test, and that red is the signal to close
/// the backlog entry and delete this test rather than adjust it.**
#[tokio::test]
async fn known_gap_owner_id_is_interpolated_without_percent_encoding() {
    let (verdict, request_line) =
        fetch_against("200 OK", r#"{"total":1}"#, Some("u1&limit=999")).await;

    assert_eq!(verdict, PeerTotal::Total(1));
    assert_eq!(
        request_line,
        format!("GET /{ENDPOINT}?limit=1&owner_id=u1&limit=999 HTTP/1.1"),
        "GAP-CLOSED: owner_id now reaches the peer encoded. DASHBOARD-OWNER-UNENCODED-1 \
         is fixed — close the backlog entry and DELETE this characterisation test."
    );
}
