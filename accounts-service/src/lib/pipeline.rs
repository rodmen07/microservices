use serde_json::Value;

/// Fire-and-forget POST of a CRM event to the DynamoDB pipeline ingest endpoint.
///
/// Reads `PIPELINE_INGEST_URL` from the environment. If unset, returns silently.
/// Errors are logged as warnings and never propagate — pipeline emission must not
/// block or fail normal service operations.
pub fn emit_event(
    client: reqwest::Client,
    source: &'static str,
    event_type: &'static str,
    payload: Value,
) {
    let url = match std::env::var("PIPELINE_INGEST_URL") {
        Ok(u) => u,
        Err(_) => return,
    };
    let body = serde_json::json!({
        "source": source,
        "event_type": event_type,
        "payload": payload,
    });
    tokio::spawn(async move {
        if let Err(e) = client.post(&url).json(&body).send().await {
            tracing::warn!("pipeline emit failed: {e}");
        }
    });
}
