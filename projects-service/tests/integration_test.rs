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

fn make_jwt_with_roles(user_id: &str, roles: &[&str]) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let claims = json!({
        "sub": user_id,
        "iss": "auth-service",
        "exp": 9999999999u64,
        "roles": roles
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
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
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
    // list_projects returns a plain JSON array, not a paginated envelope
    let body = body_json(resp.into_body()).await;
    assert!(body.as_array().is_some());
    assert!(!body.as_array().unwrap().is_empty());
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
                    json!({ "name": "Deploy to staging", "status": "not_started" }).to_string(),
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
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["name"] == "Deploy to staging"));
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

// ── Links ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_links() {
    let app = test_app().await;
    let jwt = make_jwt();
    let project_id = create_test_project(app.clone(), &jwt).await;

    // Create link
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/links"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "link_type": "github",
                        "label": "Repository",
                        "url": "https://github.com/example/repo",
                        "sort_order": 1
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let link = body_json(resp.into_body()).await;
    assert_eq!(link["label"], "Repository");
    assert_eq!(link["project_id"], project_id);

    // List links
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{project_id}/links"))
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
        .any(|l| l["label"] == "Repository"));
}

// ── Emails ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sync_and_list_emails() {
    let app = test_app().await;
    let jwt = make_jwt();
    let project_id = create_test_project(app.clone(), &jwt).await;

    // Admin syncs an email thread
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/emails/sync"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &jwt)
                .body(Body::from(
                    json!({
                        "emails": [{
                            "thread_id": "thread-int-test-1",
                            "subject": "Kickoff notes",
                            "from_email": "client@example.com",
                            "snippet": "Notes from the kickoff call",
                            "received_at": "2026-07-01T00:00:00Z"
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let synced = body_json(resp.into_body()).await;
    assert_eq!(synced["upserted"], 1);

    // List emails
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{project_id}/emails"))
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
        .any(|e| e["subject"] == "Kickoff notes"));
}

// ── Role gating ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn client_gets_403_on_admin_routes() {
    let app = test_app().await;
    let client_jwt = make_client_jwt("client-user-403");
    let fake_id = "00000000-0000-0000-0000-000000000000";

    // Every admin-only route must reject a client token with 403 before any
    // lookup happens, so fake ids are fine here.
    let attempts: Vec<(&str, String, Option<serde_json::Value>)> = vec![
        (
            "POST",
            "/api/v1/projects".to_string(),
            Some(json!({ "account_id": "acc-403", "name": "Forbidden" })),
        ),
        (
            "PATCH",
            format!("/api/v1/projects/{fake_id}"),
            Some(json!({ "name": "Forbidden" })),
        ),
        ("DELETE", format!("/api/v1/projects/{fake_id}"), None),
        (
            "POST",
            format!("/api/v1/projects/{fake_id}/milestones"),
            Some(json!({ "name": "Forbidden" })),
        ),
        (
            "PATCH",
            format!("/api/v1/milestones/{fake_id}"),
            Some(json!({ "name": "Forbidden" })),
        ),
        ("DELETE", format!("/api/v1/milestones/{fake_id}"), None),
        (
            "POST",
            format!("/api/v1/milestones/{fake_id}/deliverables"),
            Some(json!({ "name": "Forbidden" })),
        ),
        (
            "PATCH",
            format!("/api/v1/deliverables/{fake_id}"),
            Some(json!({ "name": "Forbidden" })),
        ),
        ("DELETE", format!("/api/v1/deliverables/{fake_id}"), None),
        (
            "POST",
            format!("/api/v1/projects/{fake_id}/links"),
            Some(json!({
                "link_type": "github",
                "label": "Forbidden",
                "url": "https://example.com"
            })),
        ),
        ("DELETE", format!("/api/v1/links/{fake_id}"), None),
        (
            "POST",
            format!("/api/v1/projects/{fake_id}/emails/sync"),
            Some(json!({ "emails": [] })),
        ),
    ];

    for (method, uri, payload) in attempts {
        let builder = Request::builder()
            .method(method)
            .uri(uri.as_str())
            .header(header::AUTHORIZATION, &client_jwt);
        let request = match payload {
            Some(body) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let resp = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for client token on {method} {uri}"
        );
    }
}

#[tokio::test]
async fn cross_client_access_returns_404() {
    let app = test_app().await;
    let admin_jwt = make_jwt();
    let owner_id = "client-owner-xc";
    let intruder_jwt = make_client_jwt("client-intruder-xc");

    // Admin creates a project assigned to the owner client
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
                        "account_id": "acc-xc-test",
                        "client_user_id": owner_id,
                        "name": "Cross Client Test Project"
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

    // A different client gets 404 (not 403) to avoid leaking existence
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{id}"))
                .header(header::AUTHORIZATION, &intruder_jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Nested resources are hidden the same way
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/projects/{id}/messages"))
                .header(header::AUTHORIZATION, &intruder_jwt)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn token_without_admin_or_client_role_gets_403() {
    let app = test_app().await;
    let admin_jwt = make_jwt();
    let no_role_jwt = make_jwt_with_roles("no-role-user", &[]);
    let service_jwt = make_jwt_with_roles("search-service", &["service"]);

    // Seed real data so the 403s are about the role, not missing rows
    let project_id = create_test_project(app.clone(), &admin_jwt).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/milestones"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &admin_jwt)
                .body(Body::from(json!({ "name": "Gated Phase" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let milestone_id = body_json(resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read_uris = [
        "/api/v1/projects".to_string(),
        format!("/api/v1/projects/{project_id}"),
        format!("/api/v1/projects/{project_id}/milestones"),
        format!("/api/v1/milestones/{milestone_id}/deliverables"),
        format!("/api/v1/projects/{project_id}/messages"),
        format!("/api/v1/projects/{project_id}/links"),
        format!("/api/v1/projects/{project_id}/emails"),
    ];

    for uri in &read_uris {
        for jwt in [&no_role_jwt, &service_jwt] {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri.as_str())
                        .header(header::AUTHORIZATION, jwt)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "expected 403 for non-admin non-client token on GET {uri}"
            );
        }
    }

    // A roles [] token must not be able to post messages as a client
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/projects/{project_id}/messages"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &no_role_jwt)
                .body(Body::from(
                    json!({ "body": "should never land" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
