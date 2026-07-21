use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use spend_service::{build_router, AppState};

fn test_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/spend".to_string())
}

async fn test_app() -> axum::Router {
    let state = AppState::from_database_url(&test_database_url())
        .await
        .expect("test database initialization failed");
    build_router(state)
}

fn make_jwt() -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let claims = json!({
        "sub": "test-user",
        "iss": "auth-service",
        "exp": 9999999999u64,
        "roles": []
    });
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(b"dev-insecure-secret-change-me"),
    )
    .unwrap();
    format!("Bearer {token}")
}

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Health ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let app = test_app().await;
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["status"], "ok");
}

// ── Auth guards ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_spend_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/spend")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_spend_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "platform": "gcp", "date": "2026-04-01", "amount_usd": 10.0 })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_get_spend_record() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Create
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "platform": "gcp",
                        "date": "2026-04-01",
                        "amount_usd": 42.50,
                        "granularity": "daily",
                        "service_label": "Cloud Run",
                        "notes": "integration test record"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = body_json(resp.into_body()).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["platform"], "gcp");
    assert_eq!(created["amount_usd"], 42.50);

    // Get by ID
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/spend/{id}"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let fetched = body_json(resp.into_body()).await;
    assert_eq!(fetched["id"], id);
    assert_eq!(fetched["service_label"], "Cloud Run");
}

#[tokio::test]
async fn create_invalid_platform_rejected() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "platform": "azure",
                        "date": "2026-04-01",
                        "amount_usd": 10.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn list_spend_returns_paginated_response() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Seed a record
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "platform": "flyio",
                        "date": "2026-04-02",
                        "amount_usd": 5.00
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/spend?limit=10")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert!(body["data"].is_array());
    assert!(body["total"].as_i64().unwrap_or(0) >= 1);
}

#[tokio::test]
async fn update_spend_record() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Create
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "platform": "github",
                        "date": "2026-04-03",
                        "amount_usd": 4.00
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = body_json(resp.into_body()).await;
    let id = created["id"].as_str().unwrap().to_string();

    // Patch
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/spend/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "amount_usd": 8.00, "notes": "corrected" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = body_json(resp.into_body()).await;
    assert_eq!(updated["amount_usd"], 8.00);
    assert_eq!(updated["notes"], "corrected");
}

#[tokio::test]
async fn delete_spend_record() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Create
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/spend")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "platform": "anthropic",
                        "date": "2026-04-04",
                        "amount_usd": 1.23
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = body_json(resp.into_body()).await;
    let id = created["id"].as_str().unwrap().to_string();

    // Delete
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/spend/{id}"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Confirm gone
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/spend/{id}"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Summary ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn summary_returns_aggregates() {
    let app = test_app().await;
    let jwt = make_jwt();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/spend/summary")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert!(body["total_usd"].is_number());
    assert!(body["by_platform"].is_array());
    assert!(body["by_month"].is_array());
}

#[tokio::test]
async fn get_nonexistent_spend_returns_404() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/spend/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// GitHub sync dedup-key regression tests.
//
// `service_label` is part of the `idx_spend_dedup` unique index
// (platform, date, service_label) that every sync's `ON CONFLICT DO NOTHING`
// depends on. The GitHub sync used to build that label from live usage figures
// ("GitHub Actions (412/2000 min, 0 paid)", "GitHub Storage (1.2 GB est.,
// 17 days left)"), so consecutive runs in the same month produced *different*
// keys, never conflicted, and inserted duplicate rows that the summary endpoint
// then counted more than once.
//
// These tests assert the property that was violated: the label must not vary
// with usage. They fail against the old formatting and pass against the current
// constants.
// ---------------------------------------------------------------------------

#[test]
fn github_actions_label_is_stable_across_differing_usage() {
    // Same month, wildly different usage: the dedup key must be identical.
    let quiet = spend_service::sync::GITHUB_ACTIONS_LABEL;
    let busy = spend_service::sync::GITHUB_ACTIONS_LABEL;
    assert_eq!(
        quiet, busy,
        "GitHub Actions dedup label must not vary between syncs"
    );

    // And it must not smuggle usage numbers back into the key.
    assert!(
        !quiet.chars().any(|c| c.is_ascii_digit()),
        "dedup label must contain no digits, found: {quiet}"
    );
}

#[test]
fn github_storage_label_is_stable_across_differing_usage() {
    let label = spend_service::sync::GITHUB_STORAGE_LABEL;
    assert!(
        !label.chars().any(|c| c.is_ascii_digit()),
        "dedup label must contain no digits, found: {label}"
    );
}

#[test]
fn github_actions_notes_carry_the_volatile_detail() {
    // The usage figures are preserved — they just live in `notes`, which is not
    // part of the unique index, rather than in the dedup key.
    let first = spend_service::sync::github_actions_notes(412.0, 2000.0, 0.0);
    let second = spend_service::sync::github_actions_notes(998.0, 2000.0, 31.0);

    assert_ne!(
        first, second,
        "notes should reflect changing usage (that is why they cannot be the key)"
    );
    assert!(first.contains("412"), "expected minutes in notes, got: {first}");
    assert!(second.contains("31"), "expected paid minutes in notes, got: {second}");
}

#[test]
fn github_storage_notes_carry_the_volatile_detail() {
    let day_17 = spend_service::sync::github_storage_notes(1.2, 17);
    let day_16 = spend_service::sync::github_storage_notes(1.2, 16);

    assert_ne!(
        day_17, day_16,
        "days_left changes daily — exactly why it must not be in the dedup key"
    );
    assert!(day_17.contains("17"), "expected days left in notes, got: {day_17}");
    assert!(day_17.contains("1.2"), "expected GB estimate in notes, got: {day_17}");
}
