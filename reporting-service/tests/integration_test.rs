use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use reporting_service::{build_router, AppState};
use uuid::Uuid;

/// Returns (app, unique_user_id) so each test has an isolated user namespace.
async fn test_app() -> (axum::Router, String) {
    // Ensure auth secret matches the test token signer.
    std::env::set_var("AUTH_JWT_SECRET", "dev-insecure-secret-change-me");

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/reports".to_string());
    let state = AppState::from_database_url(&url)
        .await
        .expect("test DB failed");
    let user_id = Uuid::new_v4().to_string();
    (build_router(state), user_id)
}

fn make_jwt_for(sub: &str, roles: &[&str]) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde_json::json;

    let claims = json!({
        "sub": sub,
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

fn make_jwt(sub: &str) -> String {
    make_jwt_for(sub, &[])
}

fn make_admin_jwt(sub: &str) -> String {
    make_jwt_for(sub, &["admin"])
}

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_returns_ok() {
    let (app, _) = test_app().await;
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
async fn list_reports_requires_auth() {
    let (app, _) = test_app().await;
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
    let (app, _) = test_app().await;
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
async fn dashboard_view_endpoint_user_and_admin() {
    let (app, uid) = test_app().await;
    let user_auth = make_jwt(&uid);
    let admin_auth = make_admin_jwt(&uid);

    // create one metric with current user marker
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &admin_auth)
                .body(Body::from(
                    json!({"name": "User Report", "metric": "test-user"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_resp.status(), StatusCode::CREATED);

    // user fetch should include at least 1 report via metric match
    let user_dash = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard")
                .header(header::AUTHORIZATION, &user_auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(user_dash.status(), StatusCode::OK);

    let user_body = body_json(user_dash.into_body()).await;
    assert!(user_body["reports"].as_i64().unwrap() >= 1);

    // admin query for specific user_id should include same reports count
    let admin_dash = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/dashboard?user_id={uid}"))
                .header(header::AUTHORIZATION, &admin_auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_dash.status(), StatusCode::OK);
    let admin_body = body_json(admin_dash.into_body()).await;
    assert!(admin_body["reports"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn create_report_and_check_dashboard() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);

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
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);

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

#[tokio::test]
async fn get_report_found() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"name": "Find Me", "metric": "conversions"}).to_string(),
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
                .uri(format!("/api/v1/reports/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["id"], id);
    assert_eq!(body["metric"], "conversions");
}

#[tokio::test]
async fn get_report_not_found_is_404() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_reports_returns_array() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"name": "List Report", "metric": "signups"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|r| r["name"] == "List Report"));
}

#[tokio::test]
async fn delete_report_returns_204() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(
                    json!({"name": "Delete Me", "metric": "churn"}).to_string(),
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
                .uri(format!("/api/v1/reports/{id}"))
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
                .uri(format!("/api/v1/reports/{id}"))
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_report_not_found_is_404() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_report_missing_required_fields_is_422() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"name": "", "metric": "x"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn update_report_not_found_is_404() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from(json!({"name": "Ghost"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_auth_token_is_401() {
    let (app, _) = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports")
                .header(header::AUTHORIZATION, "Bearer garbage.token.here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_report_requires_auth_is_401() {
    let (app, _) = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_report_requires_auth_is_401() {
    let (app, _) = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"name": "Unauthorized Update"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_report_requires_auth_is_401() {
    let (app, _) = test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_report_invalid_json_is_400() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from("{invalid json}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_report_invalid_json_is_400() {
    let (app, uid) = test_app().await;
    let auth = make_jwt(&uid);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/reports/00000000-0000-0000-0000-000000000000")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, &auth)
                .body(Body::from("{invalid json}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
