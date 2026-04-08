use std::env;

use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;
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

pub async fn pull_gcp_billing(pool: &SqlitePool, client: &reqwest::Client) -> SyncResult {
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
            "INSERT OR IGNORE INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
             VALUES (?, 'gcp', ?, ?, 'daily', ?, 'bigquery', NULL, ?, ?)",
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

#[derive(Deserialize)]
struct FlyGraphQLResponse {
    data: Option<FlyData>,
    errors: Option<Vec<FlyError>>,
}

#[derive(Deserialize)]
struct FlyData {
    #[serde(alias = "currentUser")]
    current_user: Option<FlyCurrentUser>,
}

#[derive(Deserialize)]
struct FlyCurrentUser {
    invoices: Option<FlyInvoices>,
}

#[derive(Deserialize)]
struct FlyInvoices {
    nodes: Vec<FlyInvoice>,
}

#[derive(Deserialize)]
struct FlyInvoice {
    amount: f64,
    #[serde(alias = "invoiceDate")]
    invoice_date: Option<String>,
}

#[derive(Deserialize)]
struct FlyError {
    message: String,
}

pub async fn pull_flyio_billing(pool: &SqlitePool, client: &reqwest::Client) -> SyncResult {
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

    let query = r#"query { currentUser { invoices { nodes { amount invoiceDate } } } }"#;

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

    let gql_resp: FlyGraphQLResponse = match resp.json().await {
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

    let invoices = match gql_resp.data {
        Some(FlyData {
            current_user:
                Some(FlyCurrentUser {
                    invoices: Some(invoices),
                }),
        }) => invoices.nodes,
        _ => {
            result
                .errors
                .push("unexpected Fly.io response shape".to_string());
            return result;
        }
    };

    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    for invoice in &invoices {
        let date = match &invoice.invoice_date {
            Some(d) => {
                // Normalise to YYYY-MM-01 (first of the invoice month)
                if d.len() >= 7 {
                    format!("{}-01", &d[..7])
                } else {
                    continue;
                }
            }
            None => continue,
        };

        // Fly.io amounts are in cents
        let amount_usd = invoice.amount / 100.0;
        if amount_usd <= 0.0 {
            continue;
        }

        let id = Uuid::new_v4().to_string();

        // Use a dedup check since the partial unique index only covers rows with service_label
        match sqlx::query(
            "INSERT INTO spend_records (id, platform, date, amount_usd, granularity, service_label, source, notes, created_at, updated_at)
             SELECT ?, 'flyio', ?, ?, 'monthly', NULL, 'flyio_graphql', NULL, ?, ?
             WHERE NOT EXISTS (
                 SELECT 1 FROM spend_records WHERE platform = 'flyio' AND date = ? AND service_label IS NULL
             )",
        )
        .bind(&id)
        .bind(&date)
        .bind(amount_usd)
        .bind(&now)
        .bind(&now)
        .bind(&date)
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
                    .push(format!("insert error for invoice {date}: {e}"));
            }
        }
    }

    result
}
