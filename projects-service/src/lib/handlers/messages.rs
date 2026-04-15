use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;
use serde_json::json;

use crate::{
    app_state::AppState,
    auth::{validate_authorization_header, AuthClaims},
    models::{ApiError, CreateMessageRequest, Message, Project},
};

fn error_response(status: StatusCode, code: &str, message: &str, details: Option<serde_json::Value>) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details,
    });
    (status, body).into_response()
}

fn require_auth_with_claims(headers: &HeaderMap) -> Result<AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message(), None))
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
            None,
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found", None))?;

    if claims.has_role("client") && project.client_user_id.as_deref() != Some(&claims.sub) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "project not found",
            None,
        ));
    }
    Ok(())
}

pub async fn list_messages(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Message>>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_project_access(&state.pool, &project_id, &claims).await?;

    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, project_id, author_id, author_role, body, created_at
         FROM messages WHERE project_id = $1 ORDER BY created_at ASC",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
            None,
        )
    })?;

    tracing::debug!(project_id = %project_id, actor = %claims.sub, count = rows.len(), "list_messages ok");
    Ok(Json(rows))
}

pub async fn create_message(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<CreateMessageRequest>,
) -> Result<Response, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_project_access(&state.pool, &project_id, &claims).await?;

    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "body must not be empty",
            Some(json!({ "field": "body", "constraint": "must not be empty" })),
        ));
    }
    if body.len() > 1000 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "body exceeds maximum length",
            Some(json!({ "field": "body", "constraint": "max 1000 characters" })),
        ));
    }

    let author_role = if claims.has_role("admin") {
        "admin"
    } else {
        "client"
    };

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO messages (id, project_id, author_id, author_role, body, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&claims.sub)
    .bind(author_role)
    .bind(&body)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
            None,
        )
    })?;

    let created = sqlx::query_as::<_, Message>(
        "SELECT id, project_id, author_id, author_role, body, created_at
         FROM messages WHERE id = $1",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
            None,
        )
    })?;

    tracing::info!(message_id = %id, project_id = %project_id, actor = %claims.sub, "message created");
    Ok((StatusCode::CREATED, Json(created)).into_response())
}
