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
    models::{ApiError, CreateProjectRequest, Project, UpdateProjectRequest},
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

const VALID_STATUSES: &[&str] = &["planning", "active", "on_hold", "completed", "cancelled"];

pub async fn list_projects(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Project>>, Response> {
    let claims = require_auth_with_claims(&headers)?;

    let rows = if claims.has_role("client") {
        sqlx::query_as::<_, Project>(
            "SELECT id, account_id, client_user_id, name, description, status,
                    start_date, target_end_date, created_at, updated_at
             FROM projects WHERE client_user_id = $1 ORDER BY created_at DESC",
        )
        .bind(&claims.sub)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, Project>(
            "SELECT id, account_id, client_user_id, name, description, status,
                    start_date, target_end_date, created_at, updated_at
             FROM projects ORDER BY created_at DESC",
        )
        .fetch_all(&state.pool)
        .await
    }
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    tracing::debug!(actor = %claims.sub, count = rows.len(), "list_projects ok");
    Ok(Json(rows))
}

pub async fn get_project(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Project>, Response> {
    let claims = require_auth_with_claims(&headers)?;

    let row = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(&id)
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

    if claims.has_role("client") && row.client_user_id.as_deref() != Some(&claims.sub) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "project not found",
        ));
    }

    tracing::debug!(project_id = %id, actor = %claims.sub, "get_project ok");
    Ok(Json(row))
}

pub async fn create_project(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Response, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "name is required",
        ));
    }
    if req.account_id.trim().is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "account_id is required",
        ));
    }

    let status = req
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("active")
        .to_string();

    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "status must be one of: planning, active, on_hold, completed, cancelled",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO projects (id, account_id, client_user_id, name, description, status,
                               start_date, target_end_date, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&id)
    .bind(&req.account_id)
    .bind(&req.client_user_id)
    .bind(&name)
    .bind(&req.description)
    .bind(&status)
    .bind(&req.start_date)
    .bind(&req.target_end_date)
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

    let created = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
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

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "projects",
        "project.created",
        serde_json::to_value(&created).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "project",
        created.id.clone(),
        created.name.clone(),
        format!(
            "status: {} | account_id: {}",
            created.status, created.account_id
        ),
    );

    tracing::info!(project_id = %id, actor = %claims.sub, "project created");
    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_project(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<Project>, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let existing = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(&id)
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

    let name = match req.name {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "name cannot be empty",
                ));
            }
            t
        }
        None => existing.name.clone(),
    };

    let status = match req.status {
        Some(v) => {
            let t = v.trim().to_string();
            if !VALID_STATUSES.contains(&t.as_str()) {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "status must be one of: planning, active, on_hold, completed, cancelled",
                ));
            }
            t
        }
        None => existing.status.clone(),
    };

    let description = req.description.or(existing.description);
    let client_user_id = req.client_user_id.or(existing.client_user_id);
    let start_date = req
        .start_date
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.start_date);
    let target_end_date = req
        .target_end_date
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.target_end_date);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE projects SET client_user_id = $1, name = $2, description = $3, status = $4,
                start_date = $5, target_end_date = $6, updated_at = $7
         WHERE id = $8",
    )
    .bind(&client_user_id)
    .bind(&name)
    .bind(&description)
    .bind(&status)
    .bind(&start_date)
    .bind(&target_end_date)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    let updated = sqlx::query_as::<_, Project>(
        "SELECT id, account_id, client_user_id, name, description, status,
                start_date, target_end_date, created_at, updated_at
         FROM projects WHERE id = $1",
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

    crate::pipeline::emit_event(
        state.http_client.clone(),
        "projects",
        "project.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "project",
        updated.id.clone(),
        updated.name.clone(),
        format!(
            "status: {} | account_id: {}",
            updated.status, updated.account_id
        ),
    );

    tracing::info!(project_id = %id, actor = %claims.sub, "project updated");
    Ok(Json(updated))
}

pub async fn delete_project(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth_with_claims(&headers)?;
    require_admin(&claims)?;

    let result = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(&id)
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
            "project not found",
        ));
    }

    crate::pipeline::delete_search_document(state.http_client.clone(), id.clone());
    tracing::info!(project_id = %id, actor = %claims.sub, "project deleted");
    Ok(StatusCode::NO_CONTENT)
}
