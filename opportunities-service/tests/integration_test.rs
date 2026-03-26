use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use opportunities_service::{build_router, AppState};

async fn test_app() -> axum::Router {
    std::env::set_var("AUTH_JWT_SECRET", "dev-insecure-secret-change-me");

    let state = AppState::from_database_url("sqlite::memory:")
        .await
        .expect("in-memory DB failed");
    build_router(state)
}

fn make_jwt() -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde_json::json;

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
}

#[tokio::test]
async fn list_opportunities_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/opportunities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_and_get_opportunity() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "account_id": "acc-001",
                        "name": "Enterprise Deal",
                        "amount": 50000.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp.into_body()).await;
    assert_eq!(created["name"], "Enterprise Deal");
    assert_eq!(created["stage"], "qualification");
    assert_eq!(created["amount"], 50000.0);

    let id = created["id"].as_str().unwrap().to_string();

    let get_resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/opportunities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let fetched = body_json(get_resp.into_body()).await;
    assert_eq!(fetched["id"], id);
}

#[tokio::test]
async fn update_opportunity_stage() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "acc-002", "name": "SMB Deal", "amount": 5000.0})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = body_json(create_resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let patch_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/opportunities/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"stage": "proposal"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch_resp.status(), StatusCode::OK);
    let updated = body_json(patch_resp.into_body()).await;
    assert_eq!(updated["stage"], "proposal");
}

#[tokio::test]
async fn delete_opportunity() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "acc-003", "name": "Pilot", "amount": 0.0}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = body_json(create_resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/opportunities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/opportunities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_opportunities_returns_array() {
    let app = test_app().await;
    let auth = make_jwt();

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "acc-list", "name": "List Deal", "amount": 1000.0})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/opportunities")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|o| o["name"] == "List Deal"));
}

#[tokio::test]
async fn create_opportunity_empty_name_is_422() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "acc-001", "name": "", "amount": 0.0}).to_string(),
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
async fn create_opportunity_empty_account_id_is_422() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "  ", "name": "Deal", "amount": 100.0}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn update_opportunity_not_found_is_404() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/opportunities/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"stage": "closed-won"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_opportunity_empty_name_is_422() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/opportunities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"account_id": "acc-001", "name": "Real Deal", "amount": 500.0})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = body_json(create_resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/opportunities/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"name": ""}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn delete_opportunity_not_found_is_404() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/opportunities/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_auth_token_is_401() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/opportunities")
                .header(header::AUTHORIZATION, "Bearer garbage.token.here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
