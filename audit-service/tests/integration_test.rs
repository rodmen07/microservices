use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use audit_service::{build_router, AppState};

fn test_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/audit".to_string())
}

async fn test_app() -> axum::Router {
    // audit-service panics without ALLOWED_ORIGINS — set a safe test value
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:5173");
    let state = AppState::new(&test_database_url(), None, None)
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

fn make_admin_jwt() -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let claims = json!({
        "sub": "admin-user",
        "iss": "auth-service",
        "exp": 9999999999u64,
        "roles": ["admin"]
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
async fn ingest_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "entity_type": "contact",
                        "entity_id": "abc",
                        "action": "created",
                        "actor_id": "user-1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_requires_admin_role() {
    let app = test_app().await;
    // Non-admin JWT → forbidden
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/audit-events")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_audit_events_no_auth_is_401() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/audit-events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Ingest ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn ingest_valid_event() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "contact",
                        "entity_id": "e1e10000-0000-0000-0000-000000000001",
                        "action": "created",
                        "actor_id": "user-test-001",
                        "entity_label": "Alice Test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["entity_type"], "contact");
    assert_eq!(body["action"], "created");
    assert_eq!(body["entity_label"], "Alice Test");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn ingest_invalid_entity_type_rejected() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "widget",
                        "entity_id": "abc",
                        "action": "created",
                        "actor_id": "user-1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn ingest_invalid_action_rejected() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "account",
                        "entity_id": "abc",
                        "action": "viewed",
                        "actor_id": "user-1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn ingest_missing_entity_id_rejected() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "contact",
                        "entity_id": "",
                        "action": "created",
                        "actor_id": "user-1"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── List (admin) ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_can_list_events() {
    let app = test_app().await;

    // Seed one event first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "account",
                        "entity_id": "acc-list-seed",
                        "action": "updated",
                        "actor_id": "admin-test"
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
                .uri("/api/v1/audit-events?limit=10")
                .header(header::AUTHORIZATION, make_admin_jwt())
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
async fn list_events_filter_by_entity_type() {
    let app = test_app().await;

    // Seed an opportunity event
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/audit-events")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::from(
                    json!({
                        "entity_type": "opportunity",
                        "entity_id": "opp-filter-test",
                        "action": "deleted",
                        "actor_id": "admin-test"
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
                .uri("/api/v1/audit-events?entity_type=opportunity")
                .header(header::AUTHORIZATION, make_admin_jwt())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let events = body["data"].as_array().unwrap();
    assert!(events.iter().all(|e| e["entity_type"] == "opportunity"));
}
