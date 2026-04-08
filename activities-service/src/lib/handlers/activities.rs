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
    auth::validate_authorization_header,
    models::{Activity, ApiError, CreateActivityRequest, UpdateActivityRequest},
};

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ApiError {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn require_auth(headers: &HeaderMap) -> Result<crate::auth::AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

pub async fn list_activities(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<Activity>>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let q = if is_admin {
        sqlx::query_as::<_, Activity>("SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities ORDER BY created_at DESC")
    } else {
        sqlx::query_as::<_, Activity>("SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE owner_id = $1 ORDER BY created_at DESC").bind(&claims.sub)
    };

    let rows = q.fetch_all(&state.pool).await.map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR",
            "database error",
        )
    })?;

    Ok(Json(rows))
}

pub async fn get_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Activity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let q = if is_admin {
        sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1",
        )
        .bind(&id)
    } else {
        sqlx::query_as::<_, Activity>(
            "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1 AND owner_id = $2",
        )
        .bind(&id)
        .bind(&claims.sub)
    };

    q.fetch_optional(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?
        .map(Json)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found"))
}

pub async fn create_activity(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateActivityRequest>,
) -> Result<Response, Response> {
    let claims = require_auth(&headers)?;
    let owner_id = claims.sub.clone();

    let activity_type = req.activity_type.trim().to_string();
    let subject = req.subject.trim().to_string();

    if activity_type.is_empty() || subject.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "activity_type and subject are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO activities
            (id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10)",
    )
    .bind(&id)
    .bind(&owner_id)
    .bind(&req.account_id)
    .bind(&req.contact_id)
    .bind(&activity_type)
    .bind(&subject)
    .bind(&req.notes)
    .bind(&req.due_at)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error"))?;

    let created = sqlx::query_as::<_, Activity>(
        "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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
        "activities",
        "activity.created",
        serde_json::to_value(&created).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "activity",
        created.id.clone(),
        created.subject.clone(),
        format!(
            "type: {} | notes: {}",
            created.activity_type,
            created.notes.as_deref().unwrap_or("-")
        ),
    );

    Ok((StatusCode::CREATED, Json(created)).into_response())
}

pub async fn update_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateActivityRequest>,
) -> Result<Json<Activity>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let existing = {
        let mut q = sqlx::query_as::<_, Activity>(
            if is_admin {
                "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1"
            } else {
                "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at, completed, created_at, updated_at FROM activities WHERE id = $1 AND owner_id = $2"
            },
        )
        .bind(&id);

        if !is_admin {
            q = q.bind(&claims.sub);
        }

        q.fetch_optional(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "activity not found")
            })?
    };

    let activity_type = match req.activity_type {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "activity_type cannot be empty",
                ));
            }
            t
        }
        None => existing.activity_type.clone(),
    };

    let subject = match req.subject {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "subject cannot be empty",
                ));
            }
            t
        }
        None => existing.subject.clone(),
    };

    let notes = req
        .notes
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.notes);
    let due_at = req
        .due_at
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.due_at);
    let completed = req.completed.unwrap_or(existing.completed);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE activities SET activity_type = $1, subject = $2, notes = $3, due_at = $4,
         completed = $5, updated_at = $6 WHERE id = $7",
    )
    .bind(&activity_type)
    .bind(&subject)
    .bind(&notes)
    .bind(&due_at)
    .bind(completed)
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

    let updated = sqlx::query_as::<_, Activity>(
        "SELECT id, owner_id, account_id, contact_id, activity_type, subject, notes, due_at,
                completed, created_at, updated_at
         FROM activities WHERE id = $1",
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
        "activities",
        "activity.updated",
        serde_json::to_value(&updated).unwrap_or_default(),
    );
    crate::pipeline::index_search_document(
        state.http_client.clone(),
        "activity",
        updated.id.clone(),
        updated.subject.clone(),
        format!(
            "type: {} | notes: {}",
            updated.activity_type,
            updated.notes.as_deref().unwrap_or("-")
        ),
    );

    Ok(Json(updated))
}

pub async fn delete_activity(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let result = if is_admin {
        sqlx::query("DELETE FROM activities WHERE id = $1")
            .bind(&id)
            .execute(&state.pool)
            .await
    } else {
        sqlx::query("DELETE FROM activities WHERE id = $1 AND owner_id = $2")
            .bind(&id)
            .bind(&claims.sub)
            .execute(&state.pool)
            .await
    }
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
            "activity not found",
        ));
    }

    crate::pipeline::delete_search_document(state.http_client.clone(), id);
    Ok(StatusCode::NO_CONTENT)
}
