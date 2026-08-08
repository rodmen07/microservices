//! First behaviour coverage for the CORS trust boundary (`build_cors_layer`).
//!
//! `build_cors_layer` decides whether a browser is allowed to talk to this
//! service at all. It is the only thing standing between the deployed
//! infraportal frontend (GitHub Pages, a different origin) and every write
//! endpoint here, and until this file it had **no test in any of the eleven
//! workspace services** — `grep -rl cors */tests/` returned nothing.
//!
//! Why that gap is dangerous rather than merely untidy: a broken CORS policy
//! is invisible to everything this repo already checks. The service still
//! boots, `/health` still returns 200, so the post-deploy health smoke test
//! added in PR #111 passes, every `curl` probe passes, and every
//! server-to-server caller keeps working. Only a real browser notices, and
//! only in production. That is not hypothetical: go-gateway shipped exactly
//! this defect (backlog bug GW-5, severity HIGH) because its `ALLOWED_ORIGINS`
//! was never set, and it went unnoticed until someone probed the header by
//! hand.
//!
//! These tests assert on the HTTP headers the real production layer emits,
//! never on the presence of a token in the source, so a comment or a dead
//! declaration cannot satisfy them. No database is involved: a CORS preflight
//! is answered by the layer itself and never reaches a handler, and the probe
//! route below is a constant.

use std::sync::{Mutex, MutexGuard, OnceLock};

use axum::{
    body::Body,
    http::{header, Method, Request},
    routing::get,
    Router,
};
use tower::ServiceExt;

use projects_service::router::build_cors_layer;

const ALLOWED_ORIGINS: &str = "ALLOWED_ORIGINS";

/// The origin the deployed frontend actually calls from, taken from the
/// repository variable `ALLOWED_ORIGINS` that `rust.yml` feeds to
/// `gcloud run deploy --set-env-vars`.
const PORTAL_ORIGIN: &str = "https://rodmen07.github.io";
const GATEWAY_ORIGIN: &str = "https://go-gateway-5gcrg4oiza-uc.a.run.app";
const HOSTILE_ORIGIN: &str = "https://evil.example.com";

/// `ALLOWED_ORIGINS` is process-global state, so every test that sets it must
/// take this lock before building its layer. The lock is deliberately
/// poison-tolerant: `wildcard_allowed_origins_is_refused` panics on purpose
/// while holding it, and a poisoned lock would cascade that one intentional
/// panic into spurious failures in every other test.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Builds the **production** CORS layer under `origins`, wrapped around a
/// stateless probe route.
///
/// The env var is read by `build_cors_layer` at call time and the resulting
/// allowlist is captured in the returned layer, so the caller may drop the
/// env lock as soon as this returns.
fn cors_app(origins: Option<&str>) -> Router {
    match origins {
        Some(value) => std::env::set_var(ALLOWED_ORIGINS, value),
        None => std::env::remove_var(ALLOWED_ORIGINS),
    }
    Router::new()
        .route("/probe", get(|| async { "ok" }))
        .layer(build_cors_layer())
}

/// A CORS preflight: what a browser sends before any PATCH/DELETE, and before
/// any GET carrying a non-simple header. If this is not authorised, the real
/// request is never sent at all.
fn preflight(origin: &str, request_method: &str) -> Request<Body> {
    Request::builder()
        .method(Method::OPTIONS)
        .uri("/probe")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, request_method)
        .body(Body::empty())
        .expect("build preflight request")
}

fn simple_get(origin: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri("/probe")
        .header(header::ORIGIN, origin)
        .body(Body::empty())
        .expect("build simple GET request")
}

/// The grant a browser looks for. `None` means the browser blocks the call.
fn allow_origin_grant(response: &axum::response::Response) -> Option<String> {
    response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .map(|value| {
            value
                .to_str()
                .expect("access-control-allow-origin must be valid ASCII")
                .to_string()
        })
}

#[tokio::test]
async fn preflight_from_an_allowlisted_origin_is_authorised() {
    let guard = env_lock();
    let app = cors_app(Some(&format!("{PORTAL_ORIGIN},{GATEWAY_ORIGIN}")));
    drop(guard);

    let response = app
        .oneshot(preflight(PORTAL_ORIGIN, "PATCH"))
        .await
        .expect("preflight must produce a response");

    assert_eq!(
        allow_origin_grant(&response).as_deref(),
        Some(PORTAL_ORIGIN),
        "the deployed frontend origin must be granted access; without this header \
         the browser blocks every cross-origin call to this service while curl and \
         every server-to-server caller keep working (the GW-5 failure mode)"
    );

    let allowed_methods = response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_METHODS)
        .map(|value| value.to_str().expect("valid ASCII").to_string())
        .unwrap_or_default();

    // GET and POST are "simple" methods a browser may send without a grant,
    // so they prove nothing. PATCH and DELETE are the ones that genuinely
    // require preflight approval, and this router serves both.
    for method in ["PATCH", "DELETE"] {
        assert!(
            allowed_methods.contains(method),
            "the router serves {method} but the CORS policy does not allow it, so \
             every {method} from a browser is blocked before it is ever sent; \
             access-control-allow-methods was {allowed_methods:?}"
        );
    }
}

#[tokio::test]
async fn simple_request_from_an_allowlisted_origin_is_authorised() {
    let guard = env_lock();
    let app = cors_app(Some(PORTAL_ORIGIN));
    drop(guard);

    let response = app
        .oneshot(simple_get(PORTAL_ORIGIN))
        .await
        .expect("GET must produce a response");

    assert_eq!(
        allow_origin_grant(&response).as_deref(),
        Some(PORTAL_ORIGIN),
        "a simple GET from the allowlisted frontend origin must carry the grant \
         header, otherwise the browser discards the body it just received"
    );
}

#[tokio::test]
async fn preflight_from_an_unlisted_origin_gets_no_grant() {
    let guard = env_lock();
    let app = cors_app(Some(&format!("{PORTAL_ORIGIN},{GATEWAY_ORIGIN}")));
    drop(guard);

    let response = app
        .oneshot(preflight(HOSTILE_ORIGIN, "DELETE"))
        .await
        .expect("preflight must produce a response");

    assert_eq!(
        allow_origin_grant(&response),
        None,
        "an origin absent from ALLOWED_ORIGINS must receive no grant; if this \
         fails the allowlist is decorative and any site can drive this API from \
         a victim's browser"
    );
}

#[test]
fn wildcard_allowed_origins_is_refused() {
    let _guard = env_lock();
    let panic = std::panic::catch_unwind(|| {
        // `catch_unwind` keeps the intentional panic from aborting the suite,
        // and lets the message itself be asserted rather than merely the fact
        // that something unwound.
        let _ = cors_app(Some("*"));
    })
    .expect_err("ALLOWED_ORIGINS=* must be refused, not silently accepted");

    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();

    assert!(
        message.contains("ALLOWED_ORIGINS"),
        "the refusal must name the offending variable so an operator can act on \
         it; panic message was {message:?}"
    );
}

/// CHARACTERISATION TEST — pins behaviour that is WRONG on purpose.
///
/// Tracks backlog bug **CORS-SPLIT-1**. With `ALLOWED_ORIGINS` unset, this
/// service (and the seven others in its group) returns a bare `CorsLayer` and
/// carries on: it boots, it serves, `/health` answers 200, the post-deploy
/// smoke test is happy — and every browser call is silently blocked. The
/// other three services (accounts, audit, contacts) `panic!` on the same
/// input and refuse to start, which is loud and diagnosable.
///
/// This asserts what the code does TODAY, not what it should do. **When the
/// eleven services are unified on the fail-closed behaviour this test MUST go
/// red**, and that red is the signal to close CORS-SPLIT-1 and delete this
/// test — not to weaken it.
#[tokio::test]
async fn known_gap_unset_allowed_origins_silently_disables_cors() {
    let guard = env_lock();
    // Built inside `catch_unwind` so that BOTH plausible fixes redden this
    // test with a legible message: unifying on the fail-closed `panic!` trips
    // the assertion below, and handing out a default grant trips the header
    // assertion further down.
    let built = std::panic::catch_unwind(|| cors_app(None));
    drop(guard);

    let app = match built {
        Ok(app) => app,
        Err(_) => panic!(
            "GAP-CLOSED: unset ALLOWED_ORIGINS now refuses to build the layer \
             instead of silently disabling CORS. That is the fail-closed \
             behaviour accounts/audit/contacts already have — close backlog bug \
             CORS-SPLIT-1 and delete this test rather than adjusting it."
        ),
    };

    let response = app
        .oneshot(preflight(PORTAL_ORIGIN, "PATCH"))
        .await
        .expect("preflight must produce a response");

    assert_eq!(
        allow_origin_grant(&response),
        None,
        "GAP-CLOSED: unset ALLOWED_ORIGINS now produces a CORS grant. The \
         known gap this test pins has changed behaviour — re-read backlog bug \
         CORS-SPLIT-1, close it if the eleven services were deliberately \
         unified, and delete this test rather than adjusting it."
    );
}
