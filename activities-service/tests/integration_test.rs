use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use activities_service::{build_router, AppState};
fn test_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/activities".to_string())
}

async fn test_app() -> axum::Router {
    let state = AppState::from_database_url(&test_database_url())
        .await
        .expect("test database initialization failed");
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
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn list_activities_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/activities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_and_list_activity() {
    let app = test_app().await;
    let auth = make_jwt();

    // Create
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "activity_type": "call",
                        "subject": "Follow-up call"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp.into_body()).await;
    assert_eq!(created["activity_type"], "call");
    assert_eq!(created["subject"], "Follow-up call");
    assert_eq!(created["completed"], false);

    let id = created["id"].as_str().unwrap().to_string();

    // List
    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/activities")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_resp.status(), StatusCode::OK);
    let list = body_json(list_resp.into_body()).await;
    assert!(list.as_array().unwrap().iter().any(|a| a["id"] == id));
}

#[tokio::test]
async fn update_activity_completed() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "email", "subject": "Send proposal"}).to_string(),
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
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"completed": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch_resp.status(), StatusCode::OK);
    let updated = body_json(patch_resp.into_body()).await;
    assert_eq!(updated["completed"], true);
}

#[tokio::test]
async fn delete_activity() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "meeting", "subject": "Kick-off"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = body_json(create_resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let del_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    let get_resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_activity_found() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "task", "subject": "Find me"}).to_string(),
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
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["id"], id);
    assert_eq!(body["subject"], "Find me");
}

#[tokio::test]
async fn get_activity_not_found_is_404() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_activity_missing_required_fields_is_422() {
    let app = test_app().await;
    let auth = make_jwt();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "", "subject": ""}).to_string(),
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
async fn update_activity_not_found_is_404() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"completed": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_activity_empty_type_is_422() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "note", "subject": "Type test"}).to_string(),
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
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"activity_type": "  "}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn delete_activity_not_found_is_404() {
    let app = test_app().await;
    let auth = make_jwt();
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
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
                .uri("/api/v1/activities")
                .header(header::AUTHORIZATION, "Bearer garbage.token.here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_activity_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_activity_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"completed": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_activity_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/activities/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_activity_invalid_json_body_is_400() {
    let app = test_app().await;
    let auth = make_jwt();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from("this is not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_activity_empty_subject_is_422() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activities")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"activity_type": "email", "subject": "Subject for update"}).to_string(),
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
                .uri(format!("/api/v1/activities/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"subject": ""}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
