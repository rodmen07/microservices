//! Five-state cross-service rollup fetches for `GET /api/v1/dashboard`
//! (v1.18 PR 2b, slice 2).
//!
//! The dashboard fans out to four sibling services and folds each one's entity
//! total into one aggregate view. Before this increment that fan-out was
//! `async fn fetch_service_total(..) -> Result<Option<i64>, Response>`, and it
//! answered five distinct situations with three different shapes:
//!
//! | the peer ...                    | old result        | old client-visible response |
//! |---------------------------------|-------------------|-----------------------------|
//! | no URL configured               | `Ok(None)`        | that field is `null`        |
//! | answers 200 with `{"total": n}` | `Ok(Some(n))`     | that field is `n`           |
//! | answers 200 with no `total`     | `Ok(None)`        | that field is `null`        |
//! | answers 403 / 404 / 5xx         | `Ok(None)`        | that field is `null`        |
//! | refuses the connection          | `Err(..)`         | **500 for the whole view**  |
//! | answers 200 with a non-JSON body| `Err(..)`         | **500 for the whole view**  |
//!
//! The last two rows are the defect, and the split is the finding: a peer that
//! is *up enough to answer* `503` costs the caller one `null`, while the SAME
//! peer refusing the connection costs them the entire dashboard — the local
//! report count, the metric list, and the three siblings that answered
//! perfectly. Which of the two a client gets is decided by how far into the
//! outage the peer's container had scaled down, which is not a distinction any
//! caller can act on.
//!
//! It is worse than a wrong status, because the fan-out is FOUR calls and the
//! failure is disjunctive: the probability the dashboard 500s is the
//! probability that ANY ONE of four Cloud Run services is cold or restarting,
//! and the dashboard is itself the thing that wakes them.
//!
//! The old code also built a fresh `reqwest::Client::new()` per call — four per
//! request — and `reqwest` sets **no default timeout**, so a peer that accepted
//! the connection and then stopped talking hung the dashboard indefinitely.
//! [`PEER_TIMEOUT`] is the bound the sibling services have had all along
//! (`activities-service/src/lib/app_state.rs` builds its client with 5 s).
//!
//! **This is LATENT today and that is why the fix comes first.** All four
//! `*_SERVICE_URL` variables are unset on every deployed revision, so every
//! call takes [`PeerTotal::NotConfigured`] and the dashboard has been returning
//! four `null`s since the 2026-07-21 rebuild. v1.18 PR 2b's remaining slice
//! wires those variables into the deploy matrix, which is what converts a
//! fan-out that never leaves the process into one whose failure modes are
//! visible to clients — the same reason slice 1 fixed
//! `activities-service`/`contacts-service`'s states before any variable was
//! supplied. That slice did not cover this service, because the rollup is not a
//! peer *check*: it decorates a read rather than gating a write.
//!
//! ## Why the states collapse to `null` here and to distinct statuses there
//!
//! `PeerCheck` (slice 1) turns `Unreachable` into a **503** and `Rejected` into
//! a **502**, and that asymmetry with this module is deliberate rather than an
//! oversight:
//!
//! * there, the check GATES A WRITE, and folding "the peer is down" into "your
//!   `account_id` does not exist" tells the client a falsehood about their own
//!   data and leaves them no useful action;
//! * here, the fetch DECORATES A READ. There is no client action either way,
//!   the published contract already answers `null` for three of the five
//!   states, and the fourth and fifth are an operator concern — so they are
//!   reported to the operator, as a structured `tracing::warn!` per failed
//!   peer, and the aggregate degrades by exactly one field.
//!
//! The consequence a reader should not have to infer: a `null` on this view
//! does not distinguish "not configured" from "the peer is down". That is
//! unchanged from before this increment for three of the five states and is
//! recorded as `DASHBOARD-NULL-OPAQUE-1` in the backlog with a close condition,
//! rather than being settled here.

/// What one sibling rollup fetch actually observed.
///
/// Five states rather than the `Result<Option<i64>, _>` this replaces, so that
/// the two rows which used to abort the whole request are nameable, loggable,
/// and — the point — separable from the three that were always `null`.
///
/// The vocabulary deliberately mirrors
/// `activities-service::peer_check::PeerCheck` (`NotConfigured` / `Unreachable`
/// / `Rejected(status)`) so the platform describes one situation with one word
/// in both places. It has no `Absent`: a 404 from a peer's *collection* route
/// is a routing or deploy fault rather than a missing record, so it lands in
/// [`Rejected`](Self::Rejected) with its status carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerTotal {
    /// No URL is configured for this peer, so no request was made. This is the
    /// state every deployed revision takes today for all four siblings.
    NotConfigured,
    /// The peer answered 2xx with a numeric `total`.
    Total(i64),
    /// The peer answered 2xx, but the body carried no numeric `total` field —
    /// including a body that was not JSON at all, which used to be a 500.
    NoTotal,
    /// The request never produced a response: connection refused, DNS or TLS
    /// failure, or [`PEER_TIMEOUT`] expiring on a cold instance. Used to be a
    /// 500 for the whole dashboard.
    Unreachable,
    /// The peer answered with a non-2xx status. Carries it so an operator can
    /// tell a refused token (401/403) from a peer outage (5xx) from a bad route
    /// (404) without reading the peer's own logs.
    Rejected(u16),
}

impl PeerTotal {
    /// The value this verdict contributes to the aggregate.
    ///
    /// Only [`Total`](Self::Total) carries a number; every other state degrades
    /// that one field to `null` and leaves the rest of the view intact. Call
    /// sites match exhaustively rather than calling this, so a state added here
    /// forces a decision (and a log line) instead of silently joining the
    /// `null` set; it exists so the intent is readable in one place and so the
    /// suite can assert the mapping directly.
    #[must_use]
    pub fn total(self) -> Option<i64> {
        match self {
            Self::Total(n) => Some(n),
            Self::NotConfigured | Self::NoTotal | Self::Unreachable | Self::Rejected(_) => None,
        }
    }
}

/// How long one sibling rollup fetch may take before it is [`Unreachable`].
///
/// [`Unreachable`]: PeerTotal::Unreachable
///
/// Matches the 5 s the CRM services already pin on their own peer clients. The
/// dashboard makes four of these sequentially, so this also bounds the whole
/// route at ~20 s where it was previously unbounded.
pub const PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The HTTP client the dashboard's fan-out uses.
///
/// `pub` and argument-free so `tests/peer_total.rs` drives the SAME client
/// `AppState` holds, rather than building a look-alike beside it: a test that
/// constructs its own `reqwest::Client` proves nothing about whether the
/// deployed one carries a timeout, which is half of what this increment fixes.
/// (The `emit_audit` / `check_peer` precedent from v1.18 PR 2a and 2b slice 1 —
/// hoist it into a named unit and observe real execution.)
#[must_use]
pub fn build_peer_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(PEER_TIMEOUT)
        .build()
        .expect("failed to build the peer HTTP client")
}

/// Ask one sibling service for its entity total, forwarding the caller's own
/// `Authorization` header.
///
/// `base_url` is passed in rather than read here, and that is load-bearing:
/// `accounts-service/tests/deploy_env_surface.rs` — the deploy-config drift
/// guard v1.18 PR 1 shipped — derives every service's environment surface by
/// scanning `*/src/**` for `env::var("LITERAL")`, and refuses to report a
/// verdict at all when the argument is not a literal it can attribute to a
/// service. Hiding these four reads behind a parameter would remove
/// `ACCOUNTS_SERVICE_URL`, `CONTACTS_SERVICE_URL`, `OPPORTUNITIES_SERVICE_URL`
/// and `ACTIVITIES_SERVICE_URL` from exactly the census that v1.18 PR 2b's
/// remaining slice relies on, so the `env::var` stays literal, at the call
/// site, where the guard can see it. (Slice 1 learned this the hard way: its
/// first push went red on that guard.)
///
/// An empty or all-whitespace `base_url` is [`PeerTotal::NotConfigured`]. The
/// blank case matters because the formatted URL would otherwise be
/// `http:///api/v1/...`, which fails in transport and would be reported as a
/// peer outage naming a peer that was never configured.
pub async fn fetch_peer_total(
    client: &reqwest::Client,
    base_url: &str,
    endpoint: &str,
    auth_header: &str,
    owner_id: Option<&str>,
) -> PeerTotal {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return PeerTotal::NotConfigured;
    }

    // `owner_id` is admin-controlled on this route (it is `?user_id=` on
    // `GET /api/v1/dashboard`), so it is handed to reqwest as a query PAIR and
    // percent-encoded, never interpolated into the URL string. The unencoded
    // `push_str(&format!("&owner_id={owner}"))` this replaces let an admin
    // inject extra query parameters into all four internal peer calls
    // (DASHBOARD-OWNER-UNENCODED-1, settled by this change before the wiring
    // slice makes the path reachable; the encoded form is pinned by
    // `a_hostile_owner_id_reaches_the_peer_as_one_encoded_parameter`).
    let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint);
    let mut pairs: Vec<(&str, &str)> = vec![("limit", "1")];
    if let Some(owner) = owner_id {
        pairs.push(("owner_id", owner));
    }

    let resp = match client
        .get(&url)
        .query(&pairs)
        .header("Authorization", auth_header)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!("dashboard rollup for {endpoint} could not be sent: {e}");
            return PeerTotal::Unreachable;
        }
    };

    let status = resp.status();
    if !status.is_success() {
        tracing::warn!("dashboard rollup for {endpoint} returned {status}");
        return PeerTotal::Rejected(status.as_u16());
    }

    let Ok(body) = resp.json::<serde_json::Value>().await else {
        tracing::warn!("dashboard rollup for {endpoint} returned a body that was not JSON");
        return PeerTotal::NoTotal;
    };

    match body.get("total").and_then(serde_json::Value::as_i64) {
        Some(total) => PeerTotal::Total(total),
        None => {
            tracing::warn!("dashboard rollup for {endpoint} carried no numeric total");
            PeerTotal::NoTotal
        }
    }
}
