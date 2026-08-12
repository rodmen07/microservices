//! Three-state referential-integrity checks against a peer service
//! (v1.18 PR 2b, slice 1 — closes PEER-CHECK-FAILS-CLOSED-1).
//!
//! The previous shape of these checks was `async fn account_exists(..) -> bool`
//! ending in
//!
//! ```text
//! Ok(resp) => resp.status().is_success(),
//! Err(e)   => { tracing::warn!(..); false }
//! ```
//!
//! so a **transport** failure — DNS, TLS, connect refused, the 5 s client
//! timeout expiring on a cold Cloud Run instance — collapsed into the same
//! `false` as an authoritative "no such record", and the caller rendered both as
//! the documented `422 INVALID_ACCOUNT / "referenced account does not exist"`.
//! A peer answering **403** (this service's forwarded token refused by the
//! peer's own `require_admin`) collapsed in identically. A client cannot tell a
//! platform outage from bad input when both arrive as the same status and the
//! same code, and the only thing it can usefully do with each — fix the id, or
//! retry later — differs.
//!
//! A `bool` cannot express the difference, so the return type is the fix:
//! [`PeerCheck`] separates "the peer said no" from "the peer did not answer",
//! and the match at each call site is exhaustive, so a state added here forces a
//! decision at every site rather than defaulting to one of them.
//!
//! This was LATENT when it was written: `ACCOUNTS_SERVICE_URL` and
//! `CONTACTS_SERVICE_URL` are unset on every deployed revision, so the checks
//! took [`PeerCheck::NotConfigured`] (the old `Err(_) => return true` fail-open
//! arm) and had not run once since the 2026-07-21 rebuild. v1.18 PR 2b wires
//! those variables, which is what converts a check that never runs into a check
//! whose failure mode is visible to clients — hence fixing the states first, in
//! its own increment, before any variable is supplied.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::models::ApiError;

/// What a peer existence check actually observed.
///
/// The three non-trivial states are deliberately distinct rather than folded
/// into a success/failure pair: they carry different HTTP semantics for the
/// client (bad input / retry later / the platform is misconfigured) and only the
/// first of them is the caller's fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerCheck {
    /// No URL is configured for this peer, so no check was performed.
    /// Preserves the historic fail-open behaviour for local development and for
    /// any revision where the deploy has not supplied the variable.
    NotConfigured,
    /// The peer answered 2xx: the referenced record exists.
    Exists,
    /// The peer answered 404: the referenced record authoritatively does not
    /// exist. This is the ONLY state that is the caller's fault, and the only
    /// one that keeps the documented 422.
    Absent,
    /// The request never produced a response — connection refused, DNS or TLS
    /// failure, or the client timeout expiring.
    Unreachable,
    /// The peer answered, but with a status that is neither 2xx nor 404: it
    /// refused (401/403), failed (5xx), or rejected the request shape. Carries
    /// the peer's status so an operator sees which.
    Rejected(u16),
}

impl PeerCheck {
    /// Whether the request may proceed past this check.
    ///
    /// Only [`Exists`](Self::Exists) and [`NotConfigured`](Self::NotConfigured)
    /// pass. Call sites match exhaustively instead of calling this, so that a
    /// new state cannot silently join the passing set; it exists for tests and
    /// for reading the intent of the two arms in one place.
    #[must_use]
    pub fn passes(self) -> bool {
        matches!(self, Self::Exists | Self::NotConfigured)
    }
}

/// `503` for a peer that never answered.
///
/// Distinct from [`rejected`] on purpose: this one is retryable and the client's
/// input may be perfectly valid, which is exactly what the old 422 denied.
#[must_use]
pub fn unreachable(field: &str, upstream: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            code: "UPSTREAM_UNAVAILABLE".to_string(),
            message: format!("{upstream} could not be reached to validate {field}"),
            details: Some(serde_json::json!({ "field": field, "upstream": upstream })),
        }),
    )
        .into_response()
}

/// `502` for a peer that answered with a status this service cannot act on.
///
/// Usually the forwarded token being refused by the peer's own role gate, which
/// is a platform misconfiguration rather than a client error; the peer's status
/// is echoed in `details.upstream_status` so that is diagnosable from the
/// response alone.
#[must_use]
pub fn rejected(field: &str, upstream: &str, upstream_status: u16) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(ApiError {
            code: "UPSTREAM_REJECTED".to_string(),
            message: format!("{upstream} refused the validation request for {field}"),
            details: Some(serde_json::json!({
                "field": field,
                "upstream": upstream,
                "upstream_status": upstream_status,
            })),
        }),
    )
        .into_response()
}

/// Ask `url_var`'s service whether `id` exists under `path`, forwarding the
/// caller's own `Authorization` header.
///
/// `pub` so `tests/peer_check.rs` can drive the real function against a stub
/// rather than grepping this file for the call (the `emit_audit` precedent from
/// v1.18 PR 2a).
pub async fn check_peer(
    client: &reqwest::Client,
    url_var: &str,
    path: &str,
    id: &str,
    auth_header: &str,
) -> PeerCheck {
    let base_url = match std::env::var(url_var) {
        Ok(url) => url,
        Err(_) => return PeerCheck::NotConfigured,
    };
    // A matrix entry present but empty is not a configured peer. Without this
    // the formatted URL is `http:///api/v1/...`, which fails in transport and
    // would now surface as a 503 blaming a peer that was never named.
    if base_url.trim().is_empty() {
        return PeerCheck::NotConfigured;
    }

    let url = format!(
        "{}/{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_matches('/'),
        id
    );

    match client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                PeerCheck::Exists
            } else if status == reqwest::StatusCode::NOT_FOUND {
                PeerCheck::Absent
            } else {
                tracing::warn!("{url_var} validation returned {status}");
                PeerCheck::Rejected(status.as_u16())
            }
        }
        Err(e) => {
            tracing::warn!("{url_var} validation request failed: {e}");
            PeerCheck::Unreachable
        }
    }
}
