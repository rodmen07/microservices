use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use reporting_service::{AppState, build_router};

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
async fn list_reports_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_summary_requires_auth() {
    let app = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_report_and_check_dashboard() {
    let app = test_app().await;
    let auth = make_jwt();

    // Dashboard starts empty
    let dash_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports/dashboard")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let dash = body_json(dash_resp.into_body()).await;
    assert_eq!(dash["active_reports"], 0);

    // Create a report
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({
                        "name": "Monthly Revenue",
                        "metric": "revenue",
                        "dimension": "month"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp.into_body()).await;
    assert_eq!(created["metric"], "revenue");

    // Dashboard now shows 1 report
    let dash2_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports/dashboard")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let dash2 = body_json(dash2_resp.into_body()).await;
    assert_eq!(dash2["active_reports"], 1);
    assert!(dash2["core_metrics"]
        .as_array()
        .unwrap()
        .contains(&json!("revenue")));
}

#[tokio::test]
async fn update_report() {
    let app = test_app().await;
    let auth = make_jwt();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"name": "Old Name", "metric": "tasks_completed"}).to_string(),
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
                .uri(format!("/api/v1/reports/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"name": "New Name"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch_resp.status(), StatusCode::OK);
    let updated = body_json(patch_resp.into_body()).await;
    assert_eq!(updated["name"], "New Name");
    assert_eq!(updated["metric"], "tasks_completed");
}
