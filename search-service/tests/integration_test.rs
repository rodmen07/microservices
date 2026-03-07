use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use search_service::{AppState, build_router};

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
}

#[tokio::test]
async fn search_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn index_and_search_document() {
    let app = test_app().await;
    let auth = make_jwt();

    // Index a document
    let index_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/search/documents")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "entity_type": "task",
                        "entity_id": "task-001",
                        "title": "Fix critical bug",
                        "body": "There is a production issue affecting payments"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(index_resp.status(), StatusCode::CREATED);
    let indexed = body_json(index_resp.into_body()).await;
    assert_eq!(indexed["entity_type"], "task");

    // Search for it
    let search_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=payments")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(search_resp.status(), StatusCode::OK);
    let results = body_json(search_resp.into_body()).await;
    let arr = results.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["entity_id"], "task-001");

    // Search for something that doesn't match
    let empty_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=nonexistent")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let empty = body_json(empty_resp.into_body()).await;
    assert_eq!(empty.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn search_matches_title() {
    let app = test_app().await;
    let auth = make_jwt();

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/search/documents")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "entity_type": "contact",
                        "entity_id": "contact-001",
                        "title": "Alice Johnson",
                        "body": "Senior engineer at Acme Corp"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let search_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=alice")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let results = body_json(search_resp.into_body()).await;
    assert_eq!(results.as_array().unwrap().len(), 1);
    assert_eq!(results[0]["entity_id"], "contact-001");
}

#[tokio::test]
async fn delete_document() {
    let app = test_app().await;
    let auth = make_jwt();

    let index_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/search/documents")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "entity_type": "account",
                        "entity_id": "acct-001",
                        "title": "Acme Corp",
                        "body": "Enterprise customer"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = body_json(index_resp.into_body()).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/search/documents/{id}"))
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
                .uri(format!("/api/v1/search/documents/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}
