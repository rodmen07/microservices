//! Behaviour guard for the two-tier role gate on every `/api/v1` route.
//!
//! Until this suite shipped, search-service authenticated but did not
//! AUTHORIZE: any valid JWT — including a client-scoped one — could read the
//! whole index (which carries the titles and bodies of accounts, activities,
//! contacts, opportunities and projects), overwrite any document, and delete
//! documents one at a time or by source entity. It was the last of the eleven
//! services without a role check; this closes 11 of 11.
//!
//! **Why the gate has two tiers rather than one.** The other ten services
//! gate every `/api/v1` route on `admin` alone. Two routes here are called
//! service-to-service — `POST /api/v1/search/documents` and
//! `DELETE /api/v1/search/documents/by-entity/{entity_id}` — by five siblings
//! (`accounts`, `activities`, `contacts`, `opportunities`, `projects`), each
//! from its own `pipeline.rs`, and those calls are FIRE-AND-FORGET: the caller
//! spawns them and ignores the response. An admin-only gate on those two would
//! therefore stop write-through indexing SILENTLY, with green CI and a green
//! deploy. They admit `admin` OR `service`, the shape audit-service already
//! uses for machine writers; everything else is admin-only.
//!
//! **The routes are DERIVED, not hand-listed.** The corpus is parsed out of
//! this service's `openapi.yaml` `paths:` block, so a route added to the spec
//! joins the guard with no edit here, and a route the router grows without a
//! spec entry is caught by `spec_and_router_declare_the_same_route_set`. Every
//! parse step hard-fails on an empty result: a reworded or restructured spec
//! must redden this suite, never silently empty its corpus. The one hand-
//! written list is the write-through POLICY set, which is a decision and not a
//! fact about the spec — and every entry in it is asserted to exist in the
//! parsed corpus, so a renamed route reddens instead of quietly dropping out.
//!
//! **No database is required — this suite runs everywhere the crate compiles.**
//! The gate rejects before any query runs, so the app is built on a LAZY pool
//! pointed at an unroutable address (`AppState::from_pool`). That is what makes
//! the admit-side assertions honest: a request that gets past the gate falls
//! through to the database and fails there, so "not 403" means "the role gate
//! let it through" rather than "some later check happened to agree".
//!
//! **Why the discriminator is the role delta and not the message.** The same
//! request is issued at tokens differing ONLY in the roles claim, and the
//! STATUS DELTA is read. Matching on the message would make this suite a
//! second, drift-prone home of the contract; the envelope test below asserts
//! the wire shape once, separately.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use search_service::{
    auth::{ROLE_ADMIN, ROLE_SERVICE},
    build_router, AppState,
};

const OPENAPI: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.yaml"));
const ROUTER_SRC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib/router.rs"));

/// Matches the JWT secret CI exports for `cargo test` (`rust.yml`) and the one
/// `docker-compose.yml` uses locally, so the suite signs tokens the service
/// accepts in both places.
const TEST_JWT_SECRET: &[u8] = b"dev-insecure-secret-change-me";

/// A body that deserializes into `IndexDocumentRequest`, which both body-
/// carrying routes extract. It matters that it deserializes: axum runs the
/// `Json` extractor BEFORE the handler, so a body that fails to decode answers
/// 422 and the request never reaches the gate under test.
const VALID_BODY: &str =
    r#"{"entity_type":"account","entity_id":"role-gating-probe","title":"t","body":"b"}"#;

/// A path parameter value that is well-formed but cannot match a document.
const ABSENT_ID: &str = "00000000-0000-4000-8000-000000000000";

/// The write-through policy set: the routes the five CRM siblings call with a
/// service identity. Not derived from the spec, because which routes a machine
/// may call is a decision; `the_write_through_routes_are_routes_this_service_serves`
/// pins each entry against the parsed corpus so a rename cannot silently empty it.
const WRITE_THROUGH: [(&str, &str); 2] = [
    ("/api/v1/search/documents", "POST"),
    ("/api/v1/search/documents/by-entity/{entity_id}", "DELETE"),
];

/// Query strings that must be attached for the request to reach the handler at
/// all. `/api/v1/search` declares the platform's only REQUIRED query parameter,
/// and `Query<SearchQuery>` is an extractor, so it runs before the handler and
/// therefore before the gate: without `q` the answer is 400 and nothing about
/// authorization is exercised. Pinned by
/// `the_required_query_parameter_is_rejected_before_the_gate`.
const REQUIRED_QUERY: [(&str, &str); 1] = [("/api/v1/search", "q=role-gating-probe")];

// ── Spec and router parsing ───────────────────────────────────────────────────

/// One operation the spec declares: an OpenAPI path template plus its method.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Operation {
    path: String,
    method: String,
}

/// Returns the `paths:` block of the spec, asserting the anchor appears exactly
/// once at column 0 and that the block is non-empty.
fn paths_block() -> Vec<&'static str> {
    let anchors = OPENAPI.lines().filter(|l| l.trim_end() == "paths:").count();
    assert_eq!(
        anchors, 1,
        "expected exactly one top-level `paths:` anchor in openapi.yaml, found {anchors}"
    );

    let block: Vec<&str> = OPENAPI
        .lines()
        .skip_while(|l| l.trim_end() != "paths:")
        .skip(1)
        // The block ends at the next column-0 key (`components:`, `tags:`, ...).
        .take_while(|l| l.is_empty() || l.starts_with(' '))
        .collect();
    assert!(
        !block.is_empty(),
        "the openapi.yaml `paths:` block parsed as empty — the spec's shape changed"
    );
    block
}

/// Extracts every (path, method) pair the spec declares, in document order.
fn spec_operations() -> Vec<Operation> {
    const METHODS: [&str; 5] = ["get", "post", "patch", "put", "delete"];
    let mut ops = Vec::new();
    let mut current: Option<String> = None;

    for line in paths_block() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        // A path key sits at exactly two spaces of indent and starts with `/`.
        if let Some(rest) = trimmed.strip_prefix("  /") {
            if !rest.starts_with(' ') {
                if let Some(name) = rest.strip_suffix(':') {
                    current = Some(format!("/{name}"));
                    continue;
                }
            }
        }
        // A method key sits at exactly four spaces of indent.
        if let Some(rest) = trimmed.strip_prefix("    ") {
            if !rest.starts_with(' ') {
                if let Some(name) = rest.strip_suffix(':') {
                    if METHODS.contains(&name) {
                        let path = current
                            .clone()
                            .expect("method key appeared before any path key in openapi.yaml");
                        ops.push(Operation {
                            path,
                            method: name.to_uppercase(),
                        });
                    }
                }
            }
        }
    }

    assert!(
        !ops.is_empty(),
        "no operations parsed out of the openapi.yaml `paths:` block"
    );
    ops
}

/// Extracts the route literals `build_router` declares, from the router source.
///
/// Scans the whole source rather than line by line, because rustfmt breaks a
/// long `.route(...)` call across lines and a line-anchored parse would drop
/// exactly the multi-method routes. Every `.route(` occurrence must yield a
/// literal, asserted below, so a shape this parse cannot read fails loudly
/// instead of shrinking the corpus in silence.
fn router_route_literals() -> Vec<String> {
    let calls = ROUTER_SRC.matches(".route(").count();
    let mut routes = Vec::new();
    for (idx, _) in ROUTER_SRC.match_indices(".route(") {
        let tail = &ROUTER_SRC[idx..];
        let Some(open) = tail.find('"') else { continue };
        let rest = &tail[open + 1..];
        let Some(end) = rest.find('"') else { continue };
        routes.push(rest[..end].to_string());
    }
    assert!(
        !routes.is_empty(),
        "no `.route(\"...\")` literals parsed out of src/lib/router.rs"
    );
    assert_eq!(
        routes.len(),
        calls,
        "src/lib/router.rs has {calls} `.route(` calls but only {} literals could be read",
        routes.len()
    );
    routes
}

// ── App construction (no database) ────────────────────────────────────────────

fn signed_token(roles: &[&str]) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": "role-gating-suite",
        "iss": "auth-service",
        "exp": 9999999999u64,
        "roles": roles,
    });
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET),
    )
    .expect("signing the test JWT failed");
    format!("Bearer {token}")
}

fn admin_token() -> String {
    signed_token(&[ROLE_ADMIN])
}

/// The machine identity the write-through callers carry.
fn service_token() -> String {
    signed_token(&[ROLE_SERVICE])
}

/// A token that is entirely valid and simply carries neither role. A non-empty
/// roles claim is deliberate: it is a defect only the role gate can see, so a
/// gate that was removed cannot be covered for by some other rejection.
fn client_token() -> String {
    signed_token(&["client"])
}

fn test_app() -> axum::Router {
    std::env::set_var("AUTH_JWT_SECRET", "dev-insecure-secret-change-me");
    // Port 1 is syntactically valid and never listening; `connect_lazy` does not
    // dial, so construction cannot fail on a machine without a database, and any
    // request that DOES reach a query fails fast rather than hanging.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy("postgres://search:search@127.0.0.1:1/documents")
        .expect("building the lazy test pool failed");
    build_router(AppState::from_pool(pool))
}

fn concrete_uri(op: &Operation) -> String {
    let path = &op.path;
    let mut uri = if let Some(open) = path.find('{') {
        let close = path[open..]
            .find('}')
            .expect("unterminated path parameter in openapi.yaml");
        format!(
            "{}{}{}",
            &path[..open],
            ABSENT_ID,
            &path[open + close + 1..]
        )
    } else {
        path.to_string()
    };
    if let Some((_, query)) = REQUIRED_QUERY.iter().find(|(p, _)| *p == path) {
        uri = format!("{uri}?{query}");
    }
    uri
}

/// Issues `op` against the app, attaching `token` when one is given.
async fn call(op: &Operation, token: Option<&str>) -> (StatusCode, Vec<u8>) {
    let method = Method::from_bytes(op.method.as_bytes()).expect("unknown HTTP method in spec");
    let carries_body = matches!(method, Method::POST | Method::PATCH | Method::PUT);

    let mut builder = Request::builder().method(method).uri(concrete_uri(op));
    if let Some(value) = token {
        builder = builder.header(header::AUTHORIZATION, value);
    }
    let request = if carries_body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(VALID_BODY))
    } else {
        builder.body(Body::empty())
    }
    .expect("building the test request failed");

    let response = test_app()
        .oneshot(request)
        .await
        .expect("the router returned an error");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("reading the response body failed")
        .to_bytes()
        .to_vec();
    (status, bytes)
}

fn api_operations() -> Vec<Operation> {
    let ops: Vec<Operation> = spec_operations()
        .into_iter()
        .filter(|op| op.path.starts_with("/api/v1"))
        .collect();
    assert!(
        !ops.is_empty(),
        "no /api/v1 operations parsed out of openapi.yaml"
    );
    ops
}

fn is_write_through(op: &Operation) -> bool {
    WRITE_THROUGH
        .iter()
        .any(|(path, method)| *path == op.path && *method == op.method)
}

/// The admin-only tier: every `/api/v1` operation that is not write-through.
fn admin_only_operations() -> Vec<Operation> {
    let ops: Vec<Operation> = api_operations()
        .into_iter()
        .filter(|op| !is_write_through(op))
        .collect();
    assert!(
        !ops.is_empty(),
        "every /api/v1 operation classified as write-through — the tier split would be vacuous"
    );
    ops
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Parser sentinel. The probes are not `/api/v1` routes, so finding them proves
/// the parse walked the whole `paths:` block rather than one lucky prefix.
#[test]
fn the_spec_parse_sees_both_the_probes_and_the_api_routes() {
    let ops = spec_operations();
    let has =
        |path: &str, method: &str| ops.iter().any(|op| op.path == path && op.method == method);
    assert!(has("/health", "GET"), "parsed operations: {ops:?}");
    assert!(has("/ready", "GET"), "parsed operations: {ops:?}");
    assert!(has("/api/v1/search", "GET"), "parsed operations: {ops:?}");
    assert!(
        has("/api/v1/search/documents", "POST"),
        "parsed operations: {ops:?}"
    );
}

/// Drift guard between the two sources that must agree about what this service
/// serves: the committed spec and the router that actually serves it. It is the
/// only direction the behaviour tests below cannot see, since they take the
/// spec as their corpus.
#[test]
fn spec_and_router_declare_the_same_route_set() {
    let mut from_spec: Vec<String> = spec_operations().into_iter().map(|op| op.path).collect();
    from_spec.sort();
    from_spec.dedup();

    let mut from_router = router_route_literals();
    from_router.sort();
    from_router.dedup();

    assert_eq!(
        from_spec, from_router,
        "openapi.yaml and src/lib/router.rs disagree about the served route set"
    );
}

/// Pins the two hand-written policy lists against the parsed corpus, so a
/// renamed or removed route reddens here instead of silently dropping out of
/// the tier tests below.
#[test]
fn the_write_through_routes_are_routes_this_service_serves() {
    let ops = api_operations();
    for (path, method) in WRITE_THROUGH {
        assert!(
            ops.iter().any(|op| op.path == path && op.method == method),
            "the write-through policy names {method} {path}, which openapi.yaml does not declare"
        );
    }
    for (path, _) in REQUIRED_QUERY {
        assert!(
            ops.iter().any(|op| op.path == path),
            "REQUIRED_QUERY names {path}, which openapi.yaml does not declare"
        );
    }
    assert_eq!(
        admin_only_operations().len() + WRITE_THROUGH.len(),
        ops.len(),
        "the two tiers do not partition the /api/v1 operation set"
    );
}

/// The refusal half of the gate: a valid token carrying neither role is refused
/// on EVERY route, write-through included.
#[tokio::test]
async fn every_api_v1_route_refuses_a_client_token() {
    let token = client_token();
    for op in api_operations() {
        let (status, _) = call(&op, Some(&token)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} admitted a token with neither the admin nor the service role",
            op.method,
            op.path
        );
    }
}

/// The admit half for `admin`. Separated from the refusal on purpose: a runner
/// stops a test at its first failed assertion, so folding both halves into one
/// case would let the refusal's red stand in for a gate that refuses EVERYONE.
#[tokio::test]
async fn no_api_v1_route_refuses_an_admin_token() {
    let token = admin_token();
    for op in api_operations() {
        let (status, _) = call(&op, Some(&token)).await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} refused an admin token",
            op.method,
            op.path
        );
    }
}

/// The admit half for `service`, and the reason this service's gate is not the
/// sibling services' gate: the two fire-and-forget write-through routes must
/// stay open to a machine identity.
#[tokio::test]
async fn the_write_through_routes_admit_a_service_token() {
    let token = service_token();
    for op in api_operations().into_iter().filter(is_write_through) {
        let (status, body) = call(&op, Some(&token)).await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} refused a service token, which would break write-through indexing silently: {}",
            op.method,
            op.path,
            String::from_utf8_lossy(&body)
        );
    }
}

/// The other direction of the same split: `service` is not a skeleton key. A
/// machine identity may write through and nothing else, so the read and
/// administrative routes refuse it.
#[tokio::test]
async fn the_admin_only_routes_refuse_a_service_token() {
    let token = service_token();
    for op in admin_only_operations() {
        let (status, _) = call(&op, Some(&token)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} admitted a service token on the admin-only tier",
            op.method,
            op.path
        );
    }
}

/// The wire contract of the refusal, asserted separately from the status so a
/// reshaped error envelope reports itself instead of hiding behind the status.
#[tokio::test]
async fn the_refusal_carries_the_documented_error_envelope() {
    let token = client_token();
    for op in api_operations() {
        let (status, body) = call(&op, Some(&token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{} {}", op.method, op.path);
        let json: Value = serde_json::from_slice(&body)
            .unwrap_or_else(|e| panic!("{} {} body was not JSON: {e}", op.method, op.path));
        assert_eq!(
            json["code"], "FORBIDDEN",
            "{} {} envelope: {json}",
            op.method, op.path
        );
        let expected = if is_write_through(&op) {
            "admin or service role required"
        } else {
            "admin role required"
        };
        assert_eq!(
            json["message"], expected,
            "{} {} envelope: {json}",
            op.method, op.path
        );
    }
}

/// A missing token is still 401, not 403: the gate did not swallow the
/// distinction the platform's error table draws between the two.
#[tokio::test]
async fn every_api_v1_route_still_answers_401_without_a_token() {
    for op in api_operations() {
        let (status, _) = call(&op, None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{} {} did not answer 401 for an anonymous caller",
            op.method,
            op.path
        );
    }
}

/// Over-gating control: the probes Cloud Run calls must never demand a token.
/// They ping the database, so on this suite's unroutable pool they answer 503;
/// what matters is that the answer is not an auth refusal.
#[tokio::test]
async fn the_health_probes_demand_no_token() {
    let probes: Vec<Operation> = spec_operations()
        .into_iter()
        .filter(|op| !op.path.starts_with("/api/v1"))
        .collect();
    assert!(!probes.is_empty(), "no probe routes parsed out of the spec");

    for op in probes {
        let (status, _) = call(&op, None).await;
        assert!(
            status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
            "{} {} demanded a token (status {status})",
            op.method,
            op.path
        );
    }
}

/// Why `REQUIRED_QUERY` exists, pinned rather than explained: extractors run
/// before the handler, so `/api/v1/search` without its required `q` is rejected
/// by axum at 400 — from an ANONYMOUS caller, i.e. before authentication has
/// happened at all (backlog bug AUTH-AFTER-PARSE-1). A corpus driver that
/// omitted the query would be asserting against that 400 and would prove
/// nothing about the gate.
#[tokio::test]
async fn the_required_query_parameter_is_rejected_before_the_gate() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/search")
        .body(Body::empty())
        .expect("building the test request failed");
    let response = test_app()
        .oneshot(request)
        .await
        .expect("the router returned an error");
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an anonymous request missing the required `q` parameter no longer answers 400 — \
         re-check whether extractors still run before the auth check"
    );
}
