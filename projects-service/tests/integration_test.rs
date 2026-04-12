use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use projects_service::{build_router, AppState};

fn test_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/projects".to_string())
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

fn make_client_jwt(user_id: &str) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let claims = json!({
        "sub": user_id,
        "iss": "auth-service",
        "exp": 9999999999u64,
        "roles": ["client"]
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
async fn list_projects_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_project_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "account_id": "acc-1", "name": "Test Project" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Projects CRUD ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_get_project() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Create
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "account_id": "acc-test-001",
                        "name": "Integration Test Project",
                        "description": "Created by integration test",
                        "status": "active"
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
    assert_eq!(created["name"], "Integration Test Project");
    assert_eq!(created["account_id"], "acc-test-001");

    // Get by ID
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{id}"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let fetched = body_json(resp.into_body()).await;
    assert_eq!(fetched["id"], id);
    assert_eq!(fetched["description"], "Created by integration test");
}

#[tokio::test]
async fn list_projects_returns_paginated_response() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Seed a project
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "account_id": "acc-list-test", "name": "List Test Project" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects")
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
async fn update_project() {
    let app = test_app().await;
    let jwt = make_jwt();

    // Create
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "account_id": "acc-upd-test", "name": "Before Update" }).to_string(),
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
                .uri(format!("/api/v1/projects/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "name": "After Update", "status": "completed" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = body_json(resp.into_body()).await;
    assert_eq!(updated["name"], "After Update");
    assert_eq!(updated["status"], "completed");
}

#[tokio::test]
async fn get_nonexistent_project_returns_404() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, make_jwt())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Milestones ────────────────────────────────────────────────────────────────

async fn create_test_project(app: axum::Router, jwt: &str) -> String {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, jwt)
                .body(Body::from(
                    json!({ "account_id": "acc-ms-test", "name": "Milestone Test Project" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    body_json(resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn create_and_list_milestones() {
    let app = test_app().await;
    let jwt = make_jwt();
    let project_id = create_test_project(app.clone(), &jwt).await;

    // Create milestone
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/milestones"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "name": "Phase 1", "status": "pending", "sort_order": 1 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let ms = body_json(resp.into_body()).await;
    assert_eq!(ms["name"], "Phase 1");
    assert_eq!(ms["project_id"], project_id);

    // List milestones
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{project_id}/milestones"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = body_json(resp.into_body()).await;
    let items = list.as_array().unwrap();
    assert!(items.iter().any(|m| m["name"] == "Phase 1"));
}

// ── Deliverables ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_deliverables() {
    let app = test_app().await;
    let jwt = make_jwt();
    let project_id = create_test_project(app.clone(), &jwt).await;

    // Create milestone
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/milestones"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "name": "Deliverable Phase" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let milestone_id = body_json(resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create deliverable
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/milestones/{milestone_id}/deliverables"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "name": "Deploy to staging", "status": "pending" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let deliv = body_json(resp.into_body()).await;
    assert_eq!(deliv["name"], "Deploy to staging");

    // List deliverables
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/milestones/{milestone_id}/deliverables"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = body_json(resp.into_body()).await;
    assert!(list.as_array().unwrap().iter().any(|d| d["name"] == "Deploy to staging"));
}

// ── Messages ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_messages() {
    let app = test_app().await;
    let jwt = make_jwt();
    let project_id = create_test_project(app.clone(), &jwt).await;

    // Post a message
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/messages"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({ "body": "Integration test message body" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let msg = body_json(resp.into_body()).await;
    assert_eq!(msg["body"], "Integration test message body");
    assert_eq!(msg["project_id"], project_id);

    // List messages
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{project_id}/messages"))
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = body_json(resp.into_body()).await;
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["body"] == "Integration test message body"));
}

// ── Client access ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn client_can_read_own_project() {
    let app = test_app().await;
    let admin_jwt = make_jwt();
    let client_id = "client-user-001";
    let client_jwt = make_client_jwt(client_id);

    // Admin creates project assigned to client
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &admin_jwt)
                .body(Body::from(
                    json!({
                        "account_id": "acc-client-test",
                        "client_user_id": client_id,
                        "name": "Client Portal Project"
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

    // Client can GET their project
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{id}"))
                .header(header::AUTHORIZATION, &client_jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let fetched = body_json(resp.into_body()).await;
    assert_eq!(fetched["client_user_id"], client_id);
}
