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
    models::{ApiError, CreateProjectLinkRequest, Project, ProjectLink},
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

fn require_admin(claims: &AuthClaims) -> Result<(), Response> {
    if claims.has_role("admin") {
        Ok(())
    } else {
        Err(error_response(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "admin role required",
            None,
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

pub async fn list_links(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<ProjectLink>>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_project_access(&state.pool, &project_id, &claims).await?;

    let rows = sqlx::query_as::<_, ProjectLink>(
        "SELECT id, project_id, link_type, label, url, sort_order, created_at, updated_at
         FROM project_links WHERE project_id = $1 ORDER BY sort_order ASC, created_at ASC",
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

    Ok(Json(rows))
}

pub async fn create_link(
    headers: HeaderMap,
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<CreateProjectLinkRequest>,
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
            None,
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found", None))?;

    let label = req.label.trim().to_string();
    let url = req.url.trim().to_string();
    let link_type = req.link_type.trim().to_string();

    if link_type.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "link_type must not be empty",
            Some(json!({ "field": "link_type", "constraint": "must not be empty" })),
        ));
    }
    if link_type.len() > 255 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "link_type exceeds maximum length",
            Some(json!({ "field": "link_type", "constraint": "max 255 characters" })),
        ));
    }

    if label.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "label must not be empty",
            Some(json!({ "field": "label", "constraint": "must not be empty" })),
        ));
    }
    if label.len() > 255 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "label exceeds maximum length",
            Some(json!({ "field": "label", "constraint": "max 255 characters" })),
        ));
    }

    if url.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "url must not be empty",
            Some(json!({ "field": "url", "constraint": "must not be empty" })),
        ));
    }
    if url.len() > 2048 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "url exceeds maximum length",
            Some(json!({ "field": "url", "constraint": "max 2048 characters" })),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let sort_order = req.sort_order.unwrap_or(0);

    sqlx::query(
        "INSERT INTO project_links (id, project_id, link_type, label, url, sort_order,
                                    created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&link_type)
    .bind(&label)
    .bind(&url)
    .bind(sort_order)
    .bind(&now)
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

    let created = sqlx::query_as::<_, ProjectLink>(
        "SELECT id, project_id, link_type, label, url, sort_order, created_at, updated_at
         FROM project_links WHERE id = $1",
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

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn delete_link(
    headers: HeaderMap,
    Path(link_id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let result = sqlx::query("DELETE FROM project_links WHERE id = $1")
        .bind(&link_id)
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

    if result.rows_affected() == 0 {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "link not found",
            None,
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
