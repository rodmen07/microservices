use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::validate_authorization_header,
    models::{
        ApiError, CreateSpendRequest, ListSpendQuery, ListSpendResponse, MonthTotal,
        PlatformTotal, SpendRecord, SpendSummary, SummaryQuery, UpdateSpendRequest,
        VALID_GRANULARITIES, VALID_PLATFORMS,
    },
    AppState,
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

fn validate_date(date: &str) -> bool {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
}

pub async fn list_spend(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<ListSpendQuery>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 200) as i64;
    let offset = params.offset.unwrap_or(0) as i64;

    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(platform) = &params.platform {
        where_clauses.push(format!("platform = ${}", param_idx));
        param_idx += 1;
        params_vec.push(platform.clone());
    }
    if let Some(date_from) = &params.date_from {
        where_clauses.push(format!("date >= ${}", param_idx));
        param_idx += 1;
        params_vec.push(date_from.clone());
    }
    if let Some(date_to) = &params.date_to {
        where_clauses.push(format!("date <= ${}", param_idx));
        param_idx += 1;
        params_vec.push(date_to.clone());
    }
    if let Some(source) = &params.source {
        where_clauses.push(format!("source = ${}", param_idx));
        param_idx += 1;
        params_vec.push(source.clone());
    }

    let mut query_base = "SELECT id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at FROM spend_records".to_string();
    let mut count_base = "SELECT COUNT(*) FROM spend_records".to_string();

    if !where_clauses.is_empty() {
        let where_stmt = format!(" WHERE {}", where_clauses.join(" AND "));
        query_base.push_str(&where_stmt);
        count_base.push_str(&where_stmt);
    }

    query_base.push_str(&format!(
        " ORDER BY date DESC, platform, service_label LIMIT ${} OFFSET ${}",
        param_idx,
        param_idx + 1
    ));

    let mut rows_query = sqlx::query_as::<_, SpendRecord>(&query_base);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_base);

    for val in &params_vec {
        rows_query = rows_query.bind(val);
        count_query = count_query.bind(val);
    }

    rows_query = rows_query.bind(limit).bind(offset);

    let rows = match rows_query.fetch_all(&state.pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_spend db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    let total = match count_query.fetch_one(&state.pool).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("list_spend count error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    Json(ListSpendResponse {
        data: rows,
        total,
        limit: limit as u32,
        offset: offset as u32,
    })
    .into_response()
}

pub async fn get_spend(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    match sqlx::query_as::<_, SpendRecord>(
        "SELECT id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at FROM spend_records WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(record)) => Json(record).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "spend record not found"),
        Err(e) => {
            tracing::error!("get_spend db error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
        }
    }
}

pub async fn create_spend(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CreateSpendRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let platform = body.platform.trim().to_lowercase();
    if !VALID_PLATFORMS.contains(&platform.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid platform".to_string(),
                details: Some(json!({ "valid_values": VALID_PLATFORMS })),
            }),
        )
            .into_response();
    }

    if !validate_date(&body.date) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "date must be YYYY-MM-DD format",
        );
    }

    if body.amount_usd < 0.0 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "amount_usd must be non-negative",
        );
    }

    let granularity = body
        .granularity
        .as_deref()
        .map(str::trim)
        .unwrap_or("daily")
        .to_string();

    if !VALID_GRANULARITIES.contains(&granularity.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "VALIDATION_ERROR".to_string(),
                message: "invalid granularity".to_string(),
                details: Some(json!({ "valid_values": VALID_GRANULARITIES })),
            }),
        )
            .into_response();
    }

    let service_label = body
        .service_label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'manual', $7, $8, $9)",
    )
    .bind(&id)
    .bind(&platform)
    .bind(&body.date)
    .bind(body.amount_usd)
    .bind(&granularity)
    .bind(&service_label)
    .bind(&notes)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("create_spend db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    }

    let record = SpendRecord {
        id,
        platform,
        date: body.date,
        amount_usd: body.amount_usd,
        granularity,
        service_label,
        source: "manual".to_string(),
        notes,
        created_at: now.clone(),
        updated_at: now,
    };

    (StatusCode::CREATED, Json(record)).into_response()
}

pub async fn update_spend(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateSpendRequest>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let existing = match sqlx::query_as::<_, SpendRecord>(
        "SELECT id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at FROM spend_records WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "spend record not found"),
        Err(e) => {
            tracing::error!("update_spend fetch error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    if existing.source != "manual" {
        return error_response(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "automated records cannot be edited",
        );
    }

    let platform = match body.platform.as_deref().map(str::trim) {
        Some(p) => {
            let p = p.to_lowercase();
            if !VALID_PLATFORMS.contains(&p.as_str()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "invalid platform".to_string(),
                        details: Some(json!({ "valid_values": VALID_PLATFORMS })),
                    }),
                )
                    .into_response();
            }
            p
        }
        None => existing.platform.clone(),
    };

    let date = match body.date.as_deref() {
        Some(d) => {
            if !validate_date(d) {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    "date must be YYYY-MM-DD format",
                );
            }
            d.to_string()
        }
        None => existing.date.clone(),
    };

    let amount_usd = match body.amount_usd {
        Some(a) => {
            if a < 0.0 {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    "amount_usd must be non-negative",
                );
            }
            a
        }
        None => existing.amount_usd,
    };

    let granularity = match body.granularity.as_deref().map(str::trim) {
        Some(g) => {
            if !VALID_GRANULARITIES.contains(&g) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "invalid granularity".to_string(),
                        details: Some(json!({ "valid_values": VALID_GRANULARITIES })),
                    }),
                )
                    .into_response();
            }
            g.to_string()
        }
        None => existing.granularity.clone(),
    };

    let service_label = match &body.service_label {
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => existing.service_label.clone(),
    };

    let notes = match &body.notes {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => existing.notes.clone(),
    };

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    match sqlx::query(
        "UPDATE spend_records SET platform = $1, date = $2, amount_usd = $3, granularity = $4, service_label = $5, notes = $6, updated_at = $7 WHERE id = $8",
    )
    .bind(&platform)
    .bind(&date)
    .bind(amount_usd)
    .bind(&granularity)
    .bind(&service_label)
    .bind(&notes)
    .bind(&now)
    .bind(&id)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("update_spend db error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    }

    let updated = SpendRecord {
        id: existing.id,
        platform,
        date,
        amount_usd,
        granularity,
        service_label,
        source: existing.source,
        notes,
        created_at: existing.created_at,
        updated_at: now,
    };

    Json(updated).into_response()
}

pub async fn delete_spend(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let existing = match sqlx::query_as::<_, SpendRecord>(
        "SELECT id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at FROM spend_records WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "NOT_FOUND", "spend record not found"),
        Err(e) => {
            tracing::error!("delete_spend fetch error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    if existing.source != "manual" {
        return error_response(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "automated records cannot be deleted",
        );
    }

    match sqlx::query("DELETE FROM spend_records WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("delete_spend db error: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error")
        }
    }
}

pub async fn get_summary(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(params): Query<SummaryQuery>,
) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let mut where_clauses = Vec::new();
    let mut params_vec: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(date_from) = &params.date_from {
        where_clauses.push(format!("date >= ${}", param_idx));
        param_idx += 1;
        params_vec.push(date_from.clone());
    }
    if let Some(date_to) = &params.date_to {
        where_clauses.push(format!("date <= ${}", param_idx));
        param_idx += 1;
        params_vec.push(date_to.clone());
    }
    let _ = param_idx; // suppress unused warning

    let where_stmt = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    // Total
    let total_sql = format!("SELECT COALESCE(SUM(amount_usd), 0.0) FROM spend_records{where_stmt}");
    let mut total_query = sqlx::query_scalar::<_, f64>(&total_sql);
    for val in &params_vec {
        total_query = total_query.bind(val);
    }
    let total_usd = match total_query.fetch_one(&state.pool).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("get_summary total error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    // By platform
    let platform_sql = format!(
        "SELECT platform, SUM(amount_usd) as total_usd FROM spend_records{where_stmt} GROUP BY platform ORDER BY platform"
    );
    let mut platform_query = sqlx::query_as::<_, (String, f64)>(&platform_sql);
    for val in &params_vec {
        platform_query = platform_query.bind(val);
    }
    let by_platform = match platform_query.fetch_all(&state.pool).await {
        Ok(rows) => rows
            .into_iter()
            .map(|(platform, total_usd)| PlatformTotal { platform, total_usd })
            .collect(),
        Err(e) => {
            tracing::error!("get_summary platform error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    // By month
    let month_sql = format!(
        "SELECT substr(date, 1, 7) as month, SUM(amount_usd) as total_usd FROM spend_records{where_stmt} GROUP BY month ORDER BY month"
    );
    let mut month_query = sqlx::query_as::<_, (String, f64)>(&month_sql);
    for val in &params_vec {
        month_query = month_query.bind(val);
    }
    let by_month = match month_query.fetch_all(&state.pool).await {
        Ok(rows) => rows
            .into_iter()
            .map(|(month, total_usd)| MonthTotal { month, total_usd })
            .collect(),
        Err(e) => {
            tracing::error!("get_summary month error: {e}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "DB_ERROR", "database error");
        }
    };

    Json(SpendSummary {
        total_usd,
        by_platform,
        by_month,
    })
    .into_response()
}

pub async fn sync_gcp(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let result = crate::sync::pull_gcp_billing(&state.pool, &state.http_client).await;
    Json(result).into_response()
}

pub async fn sync_flyio(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let result = crate::sync::pull_flyio_billing(&state.pool, &state.http_client).await;
    Json(result).into_response()
}

pub async fn sync_github(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let result = crate::sync::pull_github_billing(&state.pool, &state.http_client).await;
    Json(result).into_response()
}

pub async fn sync_aws(headers: HeaderMap, State(state): State<AppState>) -> Response {
    if let Err(resp) = require_auth(&headers) {
        return resp;
    }

    let result = crate::sync::pull_aws_billing(&state.pool, &state.http_client).await;
    Json(result).into_response()
}
