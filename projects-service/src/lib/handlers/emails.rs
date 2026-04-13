use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{validate_authorization_header, AuthClaims},
    models::{ApiError, Project, ProjectEmail, SyncEmailsRequest},
};

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

fn require_auth_with_claims(headers: &HeaderMap) -> Result<AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

fn require_admin(claims: &AuthClaims) -> Result<(), Response> {
    if claims.has_role("admin") {
        Ok(())
    } else {
        Err(error_response(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "admin role required",
        ))
    }
}

async fn require_project_access(
    pool: &sqlx::PgPool,
    project_id: &str,
    claims: &AuthClaims,
) -> Result<(), Response> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found"))?;

    if claims.has_role("client") && project.client_user_id.as_deref() != Some(&claims.sub) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "project not found",
        ));
    }
    Ok(())
}

pub async fn list_emails(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<ProjectEmail>>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_project_access(&state.pool, &project_id, &claims).await?;

    let rows = sqlx::query_as::<_, ProjectEmail>(
        "SELECT id, project_id, thread_id, subject, from_email, snippet, body_html,
                received_at, created_at, updated_at
         FROM project_emails WHERE project_id = $1 ORDER BY received_at DESC",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    Ok(Json(rows))
}

pub async fn sync_emails(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SyncEmailsRequest>,
) -> Result<Response, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    // Verify project exists
    sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(&project_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found"))?;

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut upserted = 0usize;

    for email in &req.emails {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO project_emails
                (id, project_id, thread_id, subject, from_email, snippet, body_html,
                 received_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (project_id, thread_id) DO UPDATE SET
                subject    = EXCLUDED.subject,
                from_email = EXCLUDED.from_email,
                snippet    = EXCLUDED.snippet,
                body_html  = EXCLUDED.body_html,
                received_at = EXCLUDED.received_at,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(&id)
        .bind(&project_id)
        .bind(&email.thread_id)
        .bind(&email.subject)
        .bind(&email.from_email)
        .bind(&email.snippet)
        .bind(&email.body_html)
        .bind(&email.received_at)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?;
        upserted += 1;
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "upserted": upserted })),
    )
        .into_response())
}
