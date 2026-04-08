use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    auth::{validate_authorization_header, AuthClaims},
    models::{
        ApiError, CreateReportRequest, DashboardSummary, DashboardView, SavedReport,
        UpdateReportRequest,
    },
};

// Builds a JSON error response with the given HTTP status, error code, and message
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = Json(ApiError {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    });
    (status, body).into_response()
}

// Validates the Bearer token in the request headers, returning an error response if invalid
fn require_auth(headers: &HeaderMap) -> Result<AuthClaims, Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}

// Returns a summary of saved reports including the total count and distinct metrics in use
pub async fn get_dashboard_summary(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<DashboardSummary>, Response> {
    let _claims = require_auth(&headers)?;

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reports")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?;

    let metric_rows =
        sqlx::query_scalar::<_, String>("SELECT DISTINCT metric FROM reports ORDER BY metric")
            .fetch_all(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?;

    Ok(Json(DashboardSummary {
        active_reports: count,
        core_metrics: metric_rows,
    }))
}

// Returns a rich dashboard view that can optionally be scoped to a specific user_id
async fn fetch_service_total(
    base_url: &str,
    endpoint: &str,
    auth_header: &str,
    owner_id: Option<&str>,
) -> Result<Option<i64>, Response> {
    if base_url.is_empty() {
        return Ok(None);
    }

    let mut url = format!("{}/{}?limit=1", base_url.trim_end_matches('/'), endpoint);
    if let Some(owner) = owner_id {
        url.push_str(&format!("&owner_id={}", owner));
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "HTTP_ERROR",
                "service request failed",
            )
        })?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let body: serde_json::Value = resp.json().await.map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "HTTP_ERROR",
            "invalid service response",
        )
    })?;

    Ok(body.get("total").and_then(|v| v.as_i64()))
}

pub async fn get_dashboard(
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> Result<Json<DashboardView>, Response> {
    let claims = require_auth(&headers)?;

    // Determine if this is an admin or user request.
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
    let requested_user = params.get("user_id");

    // only allow explicit user_id if admin; otherwise require own sub
    let target_user = if is_admin {
        requested_user.map(|s| s.as_str())
    } else {
        Some(claims.sub.as_str())
    };

    // Reporting-service counts
    let reports = if let Some(user_id) = target_user {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reports WHERE owner_id = $1")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?
    } else {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reports")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?
    };

    let metric_rows =
        sqlx::query_scalar::<_, String>("SELECT DISTINCT metric FROM reports ORDER BY metric")
            .fetch_all(&state.pool)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DB_ERROR",
                    "database error",
                )
            })?;

    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let accounts_url = std::env::var("ACCOUNTS_SERVICE_URL").unwrap_or_default();
    let contacts_url = std::env::var("CONTACTS_SERVICE_URL").unwrap_or_default();
    let opportunities_url = std::env::var("OPPORTUNITIES_SERVICE_URL").unwrap_or_default();
    let activities_url = std::env::var("ACTIVITIES_SERVICE_URL").unwrap_or_default();

    let owner_filter = if is_admin {
        target_user
    } else {
        Some(claims.sub.as_str())
    };

    let accounts =
        fetch_service_total(&accounts_url, "api/v1/accounts", auth_header, owner_filter).await?;
    let contacts =
        fetch_service_total(&contacts_url, "api/v1/contacts", auth_header, owner_filter).await?;
    let opportunities = fetch_service_total(
        &opportunities_url,
        "api/v1/opportunities",
        auth_header,
        owner_filter,
    )
    .await?;
    let activities = fetch_service_total(
        &activities_url,
        "api/v1/activities",
        auth_header,
        owner_filter,
    )
    .await?;

    Ok(Json(DashboardView {
        accounts,
        contacts,
        opportunities,
        activities,
        reports,
        core_metrics: metric_rows,
        stage_distribution: None,
        recent_activities: None,
    }))
}

// Returns all saved reports ordered by creation date descending
pub async fn list_reports(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Vec<SavedReport>>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let rows = if is_admin {
        sqlx::query_as::<_, SavedReport>(
            "SELECT id, name, description, metric, dimension, created_at, updated_at
             FROM reports ORDER BY created_at DESC",
        )
        .fetch_all(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?
    } else {
        sqlx::query_as::<_, SavedReport>(
            "SELECT id, name, description, metric, dimension, created_at, updated_at
             FROM reports WHERE owner_id = $1 ORDER BY created_at DESC",
        )
        .bind(&claims.sub)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "database error",
            )
        })?
    };

    Ok(Json(rows))
}

// Fetches a single saved report by ID, returning 404 if it does not exist
pub async fn get_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SavedReport>, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let row = sqlx::query_as::<_, SavedReport>(
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = $1",
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
    })?;

    let row =
        row.ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "report not found"))?;

    if !is_admin {
        // Verify ownership
        let owner_id: String = sqlx::query_scalar("SELECT owner_id FROM reports WHERE id = $1")
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

        if owner_id != claims.sub {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "report not found",
            ));
        }
    }

    Ok(Json(row))
}

// Validates and inserts a new saved report, returning the created record with HTTP 201
pub async fn create_report(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> Result<Response, Response> {
    let claims = require_auth(&headers)?;

    let name = req.name.trim().to_string();
    let metric = req.metric.trim().to_string();

    if name.is_empty() || metric.is_empty() {
        return Err(error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "name and metric are required",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "INSERT INTO reports (id, name, description, metric, dimension, owner_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&req.description)
    .bind(&metric)
    .bind(&req.dimension)
    .bind(&claims.sub)
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

    let created = sqlx::query_as::<_, SavedReport>(
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = $1",
    )
    .bind(id)
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

// Applies partial updates to an existing saved report, merging provided fields with stored values
pub async fn update_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateReportRequest>,
) -> Result<Json<SavedReport>, Response> {
    require_auth(&headers)?;

    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    let existing = sqlx::query_as::<_, SavedReport>(
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = $1",
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
    .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "report not found"))?;

    if !is_admin {
        let owner_id: String = sqlx::query_scalar("SELECT owner_id FROM reports WHERE id = $1")
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

        if owner_id != claims.sub {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "report not found",
            ));
        }
    }

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

    let metric = match req.metric {
        Some(v) => {
            let t = v.trim().to_string();
            if t.is_empty() {
                return Err(error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "VALIDATION_ERROR",
                    "metric cannot be empty",
                ));
            }
            t
        }
        None => existing.metric.clone(),
    };

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.description);
    let dimension = req
        .dimension
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .or(existing.dimension);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    sqlx::query(
        "UPDATE reports SET name = $1, description = $2, metric = $3, dimension = $4, updated_at = $5
         WHERE id = $6",
    )
    .bind(&name)
    .bind(description)
    .bind(&metric)
    .bind(dimension)
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

    let updated = sqlx::query_as::<_, SavedReport>(
        "SELECT id, name, description, metric, dimension, created_at, updated_at
         FROM reports WHERE id = $1",
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

    Ok(Json(updated))
}

// Deletes a saved report by ID, returning 204 on success or 404 if not found
pub async fn delete_report(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let claims = require_auth(&headers)?;
    let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));

    if !is_admin {
        let owner_id: Option<String> =
            sqlx::query_scalar("SELECT owner_id FROM reports WHERE id = $1")
                .bind(&id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "DB_ERROR",
                        "database error",
                    )
                })?;

        if owner_id.map(|o| o != claims.sub).unwrap_or(true) {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "report not found",
            ));
        }
    }

    let result = sqlx::query("DELETE FROM reports WHERE id = $1")
        .bind(id)
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
            "report not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
