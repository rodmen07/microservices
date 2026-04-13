use serde_json::Value;

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
        match client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => tracing::warn!(
                status = %resp.status(),
                source = %source,
                event_type = %event_type,
                "pipeline emit returned non-success status"
            ),
            Err(e) => tracing::warn!(
                error = %e,
                source = %source,
                event_type = %event_type,
                "pipeline emit failed"
            ),
        }
    });
}

pub fn index_search_document(
    client: reqwest::Client,
    entity_type: &'static str,
    entity_id: String,
    title: String,
    body: String,
) {
    let url = match std::env::var("SEARCH_SERVICE_URL") {
        Ok(u) => format!("{}/api/v1/search/documents", u.trim_end_matches('/')),
        Err(_) => return,
    };
    let token = match std::env::var("SEARCH_SERVICE_TOKEN") {
        Ok(t) => t,
        Err(_) => return,
    };
    let payload = serde_json::json!({
        "entity_type": entity_type,
        "entity_id": entity_id,
        "title": title,
        "body": body,
    });
    tokio::spawn(async move {
        for attempt in 0..3u8 {
            match client
                .post(&url)
                .bearer_auth(&token)
                .json(&payload)
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => return,
                Ok(r) => tracing::warn!("search index attempt {attempt}: status {}", r.status()),
                Err(e) => tracing::warn!("search index attempt {attempt}: {e}"),
            }
        }
        tracing::error!(
            "search index failed after 3 attempts for entity_id={}",
            payload["entity_id"]
        );
    });
}

pub fn delete_search_document(client: reqwest::Client, entity_id: String) {
    let base = match std::env::var("SEARCH_SERVICE_URL") {
        Ok(u) => u,
        Err(_) => return,
    };
    let token = match std::env::var("SEARCH_SERVICE_TOKEN") {
        Ok(t) => t,
        Err(_) => return,
    };
    let url = format!(
        "{}/api/v1/search/documents/by-entity/{}",
        base.trim_end_matches('/'),
        entity_id
    );
    tokio::spawn(async move {
        for attempt in 0..3u8 {
            match client.delete(&url).bearer_auth(&token).send().await {
                Ok(r) if r.status().is_success() || r.status() == 404 => return,
                Ok(r) => tracing::warn!("search delete attempt {attempt}: status {}", r.status()),
                Err(e) => tracing::warn!("search delete attempt {attempt}: {e}"),
            }
        }
        tracing::error!(
            "search delete failed after 3 attempts for entity_id={}",
            entity_id
        );
    });
}
