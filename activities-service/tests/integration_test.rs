use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use activities_service::{AppState, build_router};

async fn test_app() -> axum::Router {
    let state = AppState::from_database_url("sqlite::memory:")
        .await
        .expect("in-memory DB failed");
    build_router(state)
}

fn make_jwt() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
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
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
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
async fn get_nonexistent_activity_is_404() {
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
