use std::env;

use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::SyncResult;

// ---------------------------------------------------------------------------
// GCP BigQuery billing sync
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GcpServiceAccount {
    client_email: String,
    private_key: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct BqQueryResponse {
    #[serde(default)]
    rows: Vec<BqRow>,
}

#[derive(Deserialize)]
struct BqRow {
    f: Vec<BqCell>,
}

#[derive(Deserialize)]
struct BqCell {
    v: Option<String>,
}

async fn get_gcp_access_token(
    client: &reqwest::Client,
    sa: &GcpServiceAccount,
) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let claims = serde_json::json!({
        "iss": sa.client_email,
        "scope": "https://www.googleapis.com/auth/bigquery.readonly",
        "aud": "https://oauth2.googleapis.com/token",
        "iat": now,
        "exp": now + 3600,
    });

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
        .map_err(|e| format!("invalid SA private key: {e}"))?;

    let assertion = jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|e| format!("failed to sign JWT assertion: {e}"))?;

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &assertion),
        ])
        .send()
        .await
        .map_err(|e| format!("token exchange failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("token exchange returned error: {body}"));
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse token response: {e}"))?;

    Ok(token_resp.access_token)
}

pub async fn pull_gcp_billing(pool: &PgPool, client: &reqwest::Client) -> SyncResult {
    let mut result = SyncResult {
        platform: "gcp".to_string(),
        records_imported: 0,
        records_skipped: 0,
        errors: Vec::new(),
    };

    let sa_key = match env::var("GCP_BILLING_SA_KEY") {
        Ok(v) => v,
        Err(_) => {
            result
                .errors
                .push("GCP_BILLING_SA_KEY not configured".to_string());
            return result;
        }
    };

    let project_id = match env::var("GCP_BILLING_PROJECT_ID") {
        Ok(v) => v,
        Err(_) => {
            result
                .errors
                .push("GCP_BILLING_PROJECT_ID not configured".to_string());
            return result;
        }
    };

    let dataset = match env::var("GCP_BILLING_DATASET") {
        Ok(v) => v,
        Err(_) => {
            result
                .errors
                .push("GCP_BILLING_DATASET not configured".to_string());
            return result;
        }
    };

    let table = match env::var("GCP_BILLING_TABLE") {
        Ok(v) => v,
        Err(_) => {
            result
                .errors
                .push("GCP_BILLING_TABLE not configured".to_string());
            return result;
        }
    };

    let sa: GcpServiceAccount = match serde_json::from_str(&sa_key) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("failed to parse SA key JSON: {e}"));
            return result;
        }
    };

    let access_token = match get_gcp_access_token(client, &sa).await {
        Ok(t) => t,
        Err(e) => {
            result.errors.push(e);
            return result;
        }
    };

    let query_sql = format!(
        "SELECT service.description AS service_label, \
         FORMAT_DATE('%Y-%m-%d', usage_start_time) AS date, \
         SUM(cost) + SUM(IFNULL((SELECT SUM(c.amount) FROM UNNEST(credits) c), 0)) AS amount_usd \
         FROM `{dataset}.{table}` \
         WHERE usage_start_time >= TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 30 DAY) \
         GROUP BY service_label, date \
         HAVING amount_usd > 0 \
         ORDER BY date DESC, service_label"
    );

    let bq_url = format!(
        "https://bigquery.googleapis.com/bigquery/v2/projects/{project_id}/queries"
    );

    let resp = match client
        .post(&bq_url)
        .bearer_auth(&access_token)
        .json(&serde_json::json!({
            "query": query_sql,
            "useLegacySql": false,
            "maxResults": 1000,
        }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            result
                .errors
                .push(format!("BigQuery request failed: {e}"));
            return result;
        }
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        result
            .errors
            .push(format!("BigQuery returned error: {body}"));
        return result;
    }

    let bq_resp: BqQueryResponse = match resp.json().await {
        Ok(r) => r,
        Err(e) => {
            result
                .errors
                .push(format!("failed to parse BigQuery response: {e}"));
            return result;
        }
    };

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    for row in &bq_resp.rows {
        if row.f.len() < 3 {
            continue;
        }
        let service_label = row.f[0].v.as_deref().unwrap_or("Unknown");
        let date = match row.f[1].v.as_deref() {
            Some(d) => d,
            None => continue,
        };
        let amount: f64 = match row.f[2].v.as_deref().and_then(|v| v.parse().ok()) {
            Some(a) => a,
            None => continue,
        };

        let id = Uuid::new_v4().to_string();
        match sqlx::query(
            "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
             VALUES ($1, 'gcp', $2, $3, 'daily', $4, 'bigquery', NULL, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(&id)
        .bind(date)
        .bind(amount)
        .bind(service_label)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    result.records_imported += 1;
                } else {
                    result.records_skipped += 1;
                }
            }
            Err(e) => {
                result.errors.push(format!(
                    "insert error for {date}/{service_label}: {e}"
                ));
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Fly.io GraphQL billing sync
// ---------------------------------------------------------------------------
// Fly.io removed per-invoice history from their public GraphQL API.
// We now query org-level creditBalance (in cents) and record a monthly
// snapshot if there is an outstanding balance.

#[derive(Deserialize)]
struct FlyOrgsResponse {
    data: Option<FlyOrgsData>,
    errors: Option<Vec<FlyError>>,
}

#[derive(Deserialize)]
struct FlyOrgsData {
    organizations: Option<FlyOrgConnection>,
}

#[derive(Deserialize)]
struct FlyOrgConnection {
    nodes: Vec<FlyOrg>,
}

#[derive(Deserialize)]
struct FlyOrg {
    slug: String,
    #[serde(alias = "creditBalance")]
    credit_balance: i64, // cents; negative = they owe you, positive = you owe them
}

#[derive(Deserialize)]
struct FlyError {
    message: String,
}

pub async fn pull_flyio_billing(pool: &PgPool, client: &reqwest::Client) -> SyncResult {
    let mut result = SyncResult {
        platform: "flyio".to_string(),
        records_imported: 0,
        records_skipped: 0,
        errors: Vec::new(),
    };

    let token = match env::var("FLYIO_API_TOKEN") {
        Ok(v) => v,
        Err(_) => {
            result
                .errors
                .push("FLYIO_API_TOKEN not configured".to_string());
            return result;
        }
    };

    // Fly.io dropped per-invoice history; org creditBalance is the best available signal.
    let query = r#"query { organizations { nodes { slug creditBalance } } }"#;

    let resp = match client
        .post("https://api.fly.io/graphql")
        .bearer_auth(&token)
        .json(&serde_json::json!({ "query": query }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            result
                .errors
                .push(format!("Fly.io GraphQL request failed: {e}"));
            return result;
        }
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        result
            .errors
            .push(format!("Fly.io returned error: {body}"));
        return result;
    }

    let gql_resp: FlyOrgsResponse = match resp.json().await {
        Ok(r) => r,
        Err(e) => {
            result
                .errors
                .push(format!("failed to parse Fly.io response: {e}"));
            return result;
        }
    };

    if let Some(errors) = &gql_resp.errors {
        for err in errors {
            result.errors.push(format!("GraphQL error: {}", err.message));
        }
        if gql_resp.data.is_none() {
            return result;
        }
    }

    let orgs = match gql_resp.data.and_then(|d| d.organizations) {
        Some(conn) => conn.nodes,
        None => {
            result
                .errors
                .push("unexpected Fly.io response shape".to_string());
            return result;
        }
    };

    let now = Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    // Record against the first of the current month so ON CONFLICT deduplicates per-month syncs.
    let month_date = now.format("%Y-%m-01").to_string();

    for org in &orgs {
        // creditBalance > 0 means the account owes Fly.io (outstanding balance in cents).
        if org.credit_balance <= 0 {
            result.records_skipped += 1;
            continue;
        }

        let amount_usd = org.credit_balance as f64 / 100.0;
        let label = format!("Fly.io ({})", org.slug);
        let id = Uuid::new_v4().to_string();

        match sqlx::query(
            "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
             VALUES ($1, 'flyio', $2, $3, 'monthly', $4, 'flyio_graphql', NULL, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(&id)
        .bind(&month_date)
        .bind(amount_usd)
        .bind(&label)
        .bind(&now_str)
        .bind(&now_str)
        .execute(pool)
        .await
        {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    result.records_imported += 1;
                } else {
                    result.records_skipped += 1;
                }
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("insert error for org {}: {e}", org.slug));
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// GitHub billing sync — Actions minutes + storage via GitHub REST API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GitHubActionsUsage {
    total_minutes_used: f64,
    total_paid_minutes_used: f64,
    included_minutes: f64,
}

#[derive(Deserialize)]
struct GitHubStorageUsage {
    days_left_in_billing_cycle: u32,
    estimated_paid_storage_for_month: f64,
    estimated_storage_for_month: f64,
}

pub async fn pull_github_billing(pool: &PgPool, client: &reqwest::Client) -> SyncResult {
    let mut result = SyncResult {
        platform: "github".to_string(),
        records_imported: 0,
        records_skipped: 0,
        errors: Vec::new(),
    };

    let token = match env::var("GITHUB_BILLING_TOKEN") {
        Ok(v) => v,
        Err(_) => {
            result.errors.push("GITHUB_BILLING_TOKEN not configured".to_string());
            return result;
        }
    };

    let username = match env::var("GITHUB_BILLING_USERNAME") {
        Ok(v) => v,
        Err(_) => {
            result.errors.push("GITHUB_BILLING_USERNAME not configured".to_string());
            return result;
        }
    };

    let now = Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let month_date = now.format("%Y-%m-01").to_string();

    // Actions usage
    let actions_resp = client
        .get(format!("https://api.github.com/users/{username}/settings/billing/actions"))
        .bearer_auth(&token)
        .header("User-Agent", "spend-service/1.0")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;

    match actions_resp {
        Err(e) => result.errors.push(format!("GitHub Actions billing request failed: {e}")),
        Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
            // 404 = no paid usage data available (free-tier account); treat as zero spend.
            result.records_skipped += 1;
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            result.errors.push(format!("GitHub Actions billing {status}: {body}"));
        }
        Ok(r) => match r.json::<GitHubActionsUsage>().await {
            Err(e) => result.errors.push(format!("failed to parse GitHub Actions usage: {e}")),
            Ok(usage) => {
                let paid = usage.total_paid_minutes_used;
                let minutes = usage.total_minutes_used;
                // GitHub charges $0.008/min for Linux runners on paid overages
                let amount_usd = paid * 0.008;
                let label = format!(
                    "GitHub Actions ({:.0}/{:.0} min, {:.0} paid)",
                    minutes, usage.included_minutes, paid
                );
                if amount_usd > 0.0 {
                    let id = Uuid::new_v4().to_string();
                    match sqlx::query(
                        "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
                         VALUES ($1, 'github', $2, $3, 'monthly', $4, 'github_api', NULL, $5, $6)
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&id).bind(&month_date).bind(amount_usd).bind(&label).bind(&now_str).bind(&now_str)
                    .execute(pool).await
                    {
                        Ok(r) => { if r.rows_affected() > 0 { result.records_imported += 1; } else { result.records_skipped += 1; } }
                        Err(e) => result.errors.push(format!("insert error (actions): {e}")),
                    }
                } else {
                    result.records_skipped += 1;
                }
            }
        },
    }

    // Storage usage
    let storage_resp = client
        .get(format!("https://api.github.com/users/{username}/settings/billing/shared-storage"))
        .bearer_auth(&token)
        .header("User-Agent", "spend-service/1.0")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;

    match storage_resp {
        Err(e) => result.errors.push(format!("GitHub storage billing request failed: {e}")),
        Ok(r) if !r.status().is_success() => { let _ = r.text().await; } // non-fatal
        Ok(r) => match r.json::<GitHubStorageUsage>().await {
            Err(_) => {} // non-fatal
            Ok(usage) => {
                let amount_usd = usage.estimated_paid_storage_for_month;
                if amount_usd > 0.0 {
                    let label = format!(
                        "GitHub Storage ({:.1} GB est., {} days left)",
                        usage.estimated_storage_for_month, usage.days_left_in_billing_cycle
                    );
                    let id = Uuid::new_v4().to_string();
                    match sqlx::query(
                        "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
                         VALUES ($1, 'github', $2, $3, 'monthly', $4, 'github_api', NULL, $5, $6)
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&id).bind(&month_date).bind(amount_usd).bind(&label).bind(&now_str).bind(&now_str)
                    .execute(pool).await
                    {
                        Ok(r) => { if r.rows_affected() > 0 { result.records_imported += 1; } else { result.records_skipped += 1; } }
                        Err(e) => result.errors.push(format!("insert error (storage): {e}")),
                    }
                } else {
                    result.records_skipped += 1;
                }
            }
        },
    }

    result
}

// ---------------------------------------------------------------------------
// AWS Cost Explorer sync — last 30 days grouped by service (SigV4)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CostExplorerResponse {
    #[serde(rename = "ResultsByTime")]
    results_by_time: Vec<CostResult>,
}

#[derive(Deserialize)]
struct CostResult {
    #[serde(rename = "TimePeriod")]
    time_period: TimePeriod,
    #[serde(rename = "Groups")]
    groups: Vec<CostGroup>,
}

#[derive(Deserialize)]
struct TimePeriod {
    #[serde(rename = "Start")]
    start: String,
}

#[derive(Deserialize)]
struct CostGroup {
    #[serde(rename = "Keys")]
    keys: Vec<String>,
    #[serde(rename = "Metrics")]
    metrics: CostMetrics,
}

#[derive(Deserialize)]
struct CostMetrics {
    #[serde(rename = "UnblendedCost")]
    unblended_cost: CostAmount,
}

#[derive(Deserialize)]
struct CostAmount {
    #[serde(rename = "Amount")]
    amount: String,
}

pub async fn pull_aws_billing(pool: &PgPool, client: &reqwest::Client) -> SyncResult {
    let mut result = SyncResult {
        platform: "aws".to_string(),
        records_imported: 0,
        records_skipped: 0,
        errors: Vec::new(),
    };

    let access_key = match env::var("AWS_BILLING_ACCESS_KEY_ID") {
        Ok(v) => v,
        Err(_) => {
            result.errors.push("AWS_BILLING_ACCESS_KEY_ID not configured".to_string());
            return result;
        }
    };
    let secret_key = match env::var("AWS_BILLING_SECRET_ACCESS_KEY") {
        Ok(v) => v,
        Err(_) => {
            result.errors.push("AWS_BILLING_SECRET_ACCESS_KEY not configured".to_string());
            return result;
        }
    };

    let now = Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let end_date = now.format("%Y-%m-%d").to_string();
    let start_date = (now - chrono::Duration::days(30)).format("%Y-%m-%d").to_string();

    let body = serde_json::json!({
        "TimePeriod": { "Start": start_date, "End": end_date },
        "Granularity": "MONTHLY",
        "GroupBy": [{ "Type": "DIMENSION", "Key": "SERVICE" }],
        "Metrics": ["UnblendedCost"]
    })
    .to_string();

    // AWS SigV4 signing — Cost Explorer is us-east-1 only
    let region = "us-east-1";
    let host = "ce.us-east-1.amazonaws.com";
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    let body_hash = hex(&Sha256::digest(body.as_bytes()));
    let canonical_headers = format!("content-type:application/x-amz-json-1.1\nhost:{host}\nx-amz-date:{amz_date}\n");
    let signed_headers = "content-type;host;x-amz-date";
    let canonical_request = format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{body_hash}");
    let credential_scope = format!("{date_stamp}/{region}/ce/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = {
        let k1 = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_stamp.as_bytes());
        let k2 = hmac_sha256(&k1, region.as_bytes());
        let k3 = hmac_sha256(&k2, b"ce");
        hmac_sha256(&k3, b"aws4_request")
    };
    let signature = hex(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let resp = match client
        .post(format!("https://{host}/"))
        .header("Content-Type", "application/x-amz-json-1.1")
        .header("X-Amz-Date", &amz_date)
        .header("X-Amz-Target", "AWSInsightsIndexService.GetCostAndUsage")
        .header("Authorization", authorization)
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            result.errors.push(format!("AWS Cost Explorer request failed: {e}"));
            return result;
        }
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        result.errors.push(format!("AWS Cost Explorer error: {body}"));
        return result;
    }

    let cost_resp: CostExplorerResponse = match resp.json().await {
        Ok(r) => r,
        Err(e) => {
            result.errors.push(format!("failed to parse AWS Cost Explorer response: {e}"));
            return result;
        }
    };

    for period in &cost_resp.results_by_time {
        let date = if period.time_period.start.len() >= 7 {
            format!("{}-01", &period.time_period.start[..7])
        } else {
            continue;
        };

        for group in &period.groups {
            let service = group.keys.first().cloned().unwrap_or_default();
            let amount_usd: f64 = group.metrics.unblended_cost.amount.parse().unwrap_or(0.0);
            if amount_usd <= 0.0 {
                continue;
            }

            let id = Uuid::new_v4().to_string();
            match sqlx::query(
                "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
                 VALUES ($1, 'aws', $2, $3, 'monthly', $4, 'aws_cost_explorer', NULL, $5, $6)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&id).bind(&date).bind(amount_usd).bind(&service).bind(&now_str).bind(&now_str)
            .execute(pool).await
            {
                Ok(r) => { if r.rows_affected() > 0 { result.records_imported += 1; } else { result.records_skipped += 1; } }
                Err(e) => result.errors.push(format!("insert error ({service} {date}): {e}")),
            }
        }
    }

    result
}
