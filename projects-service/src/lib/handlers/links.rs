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
    models::{ApiError, CreateProjectLinkRequest, Project, ProjectLink},
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
        )
    })?
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "project not found"))?;

    let label = req.label.trim().to_string();
    let url = req.url.trim().to_string();
    let link_type = req.link_type.trim().to_string();

    if label.is_empty() || url.is_empty() || link_type.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "link_type, label, and url are required",
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
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "link not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
