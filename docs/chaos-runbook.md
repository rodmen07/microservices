# Chaos Engineering Runbook — InfraPortal

This runbook describes known failure modes for the InfraPortal microservices platform, the expected system behavior for each, and the procedure to inject and recover from each fault. Use this to validate resilience properties before releases and after infrastructure changes.

**Platform:** Google Cloud Run (us-central1) + Cloud SQL PostgreSQL 16 (us-south1)  
**Tooling required:** `gcloud`, `k6`, `psql`

---

## Table of Contents

1. [Cold Start Latency](#1-cold-start-latency)
2. [Cloud SQL Connection Exhaustion](#2-cloud-sql-connection-exhaustion)
3. [Upstream Dependency Unavailable (fail-open)](#3-upstream-dependency-unavailable-fail-open)
4. [Observaboard Unavailability](#4-observaboard-unavailability)
5. [Single Service Crash Loop](#5-single-service-crash-loop)
6. [Rollback Procedure](#6-rollback-procedure)

---

## 1. Cold Start Latency

### Context
All services run with `min_instances = 0` to minimize cost. A service that has received no traffic for ~15 minutes will scale to zero. The first request after scale-down triggers a cold start.

### Expected behavior
- Cold start adds **200–800 ms** of latency for the first request.
- Subsequent requests are unaffected (instance stays warm as long as traffic continues).
- Cloud Run's load balancer queues requests during cold start; no requests are dropped.
- The `/health` endpoint itself can trigger a warm-up.

### How to observe
```bash
# Force a cold start by scaling to zero (requires IAM permission)
gcloud run services update accounts-service \
  --region us-central1 \
  --min-instances 0 \
  --max-instances 0   # temporarily block new instances

# Wait 60 s, then restore
gcloud run services update accounts-service \
  --region us-central1 \
  --min-instances 0 \
  --max-instances 3

# Immediately hit the health endpoint and observe latency
time curl -s https://<accounts-url>/health
```

### Mitigation options
- Increase `min_instances` to 1 for latency-sensitive services — costs ~$3/month per service.
- Use a scheduled Cloud Scheduler job to ping `/health` on each service every 10 minutes.
- Accept cold start as a portfolio tradeoff — no SLA commitment currently.

### Recovery
No action required. Cold starts are transient; the system self-heals on the next warm request.

---

## 2. Cloud SQL Connection Exhaustion

### Context
The shared Cloud SQL instance (`db-f1-micro`) has a max connection limit of **25 connections**. With 11 services each holding a pool of 5 connections (`max_connections = 5`), the theoretical maximum is 55 — well above the instance limit. In practice, inactive services hold 0 connections (Cloud Run scales to zero), but a traffic burst that warms all services simultaneously could exhaust the pool.

### Expected behavior
- Services that cannot acquire a connection return **500 Internal Server Error** with `{ "code": "DB_ERROR" }`.
- The error is transient — once a connection is released, the next request succeeds.
- Services fail independently; a connection shortage in `contacts-service` does not affect `accounts-service`.

### How to inject the fault
```bash
# Connect to Cloud SQL via the Auth Proxy and hold connections
# (requires cloud-sql-proxy installed and ADC configured)
cloud-sql-proxy microservices-489413:us-south1:microservices-pg &

# Open 25 idle connections with psql to exhaust the pool
for i in $(seq 1 25); do
  psql "postgres://postgres:<pass>@localhost:5432/accounts" \
    -c "SELECT pg_sleep(120);" &
done

# In another terminal, run the load test — observe 500s
k6 run -e ACCOUNTS_URL=https://<accounts-url> scripts/load-test.js

# Kill the idle connections to recover
kill %1
```

### Mitigation options
- Upgrade Cloud SQL tier to `db-g1-small` (max 1000 connections) — costs ~$26/month.
- Add a PgBouncer connection pooler between services and Cloud SQL.
- Reduce per-service pool to 2 connections (`max_connections = 2`); 11 × 2 = 22, safely under the limit even when all services are warm.

### Current posture
Each service uses `max_connections = 5`. This is safe as long as no more than 5 services are simultaneously warm. Acceptable for a portfolio workload.

---

## 3. Upstream Dependency Unavailable (fail-open)

### Context
Several services make cross-service HTTP calls:
- `contacts-service` → `accounts-service` (account_id validation on create)
- `activities-service` → `accounts-service` and `contacts-service` (validation on create)
- `reporting-service` → `accounts`, `contacts`, `opportunities`, `activities` (dashboard aggregation)

All of these calls are **fail-open**: if the upstream service URL is not configured (`ACCOUNTS_SERVICE_URL` is empty) or the call fails with a network error, the creating service proceeds without validation.

### Expected behavior
- Contacts created without a reachable `accounts-service` are accepted and stored.
- The response does **not** include a warning that validation was skipped.
- Referential integrity is best-effort, not enforced by the DB (no FK constraint across services).

### How to inject the fault
```bash
# Point contacts-service at a non-existent URL via Cloud Run env override
gcloud run services update contacts-service \
  --region us-central1 \
  --set-env-vars "ACCOUNTS_SERVICE_URL=http://does-not-exist.invalid"

# Create a contact with a bogus account_id — should succeed (fail-open)
curl -X POST https://<contacts-url>/api/v1/contacts \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"account_id":"fake-id","first_name":"Chaos","last_name":"Test","lifecycle_stage":"lead"}'

# Restore
gcloud run services update contacts-service \
  --region us-central1 \
  --set-env-vars "ACCOUNTS_SERVICE_URL=https://<accounts-url>"
```

### Mitigation options
- Add a `validation_skipped: bool` field to create responses to make the skip visible.
- Introduce a saga/outbox pattern that re-validates in the background and tombstones invalid records.
- For portfolio purposes: document fail-open behavior and accept the tradeoff.

---

## 4. Observaboard Unavailability

### Context
`audit-service` fire-and-forgets CRM events to Observaboard (`POST /api/ingest/`) after each successful DB insert. The call is made in a `tokio::spawn` task and its result is only logged — a failure does not affect the audit event response.

### Expected behavior
- `POST /api/v1/audit-events` returns **201 Created** regardless of Observaboard availability.
- If Observaboard is down, a `WARN` log is emitted: `observaboard ingest returned 503` or `observaboard ingest failed: unreachable`.
- No audit events are lost from the audit DB; only the Observaboard mirror is incomplete.
- When Observaboard recovers, new events resume forwarding. Historical events during the outage are not backfilled automatically.

### How to inject the fault
```bash
# Point audit-service at a non-existent Observaboard URL
gcloud run services update audit-service \
  --region us-central1 \
  --update-env-vars "OBSERVABOARD_INGEST_URL=http://does-not-exist.invalid/api/ingest/"

# Ingest an audit event — should still return 201
curl -X POST https://<audit-url>/api/v1/audit-events \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"entity_type":"contact","entity_id":"chaos-test","action":"created","actor_id":"test"}'

# Inspect Cloud Run logs for the warn
gcloud logging read \
  'resource.type="cloud_run_revision" AND resource.labels.service_name="audit-service" AND textPayload:"observaboard"' \
  --limit 10 --format "value(textPayload)"

# Restore
gcloud run services update audit-service \
  --region us-central1 \
  --update-env-vars "OBSERVABOARD_INGEST_URL=https://observaboard-rodmen07.fly.dev/api/ingest/"
```

### Mitigation options
- Add an outbox table to audit-service: record failed Observaboard forwards and retry with exponential backoff.
- Current posture: fire-and-forget with warn logging is acceptable — Observaboard is a read-only observability mirror, not a system of record.

---

## 5. Single Service Crash Loop

### Context
A bad deploy (e.g., a missing env var causing a panic at startup) can cause a service to crash-loop. Cloud Run will retry with exponential backoff and eventually stop routing traffic to failed instances.

### How to simulate
```bash
# Deploy a version with a broken DATABASE_URL to trigger panic at startup
gcloud run services update accounts-service \
  --region us-central1 \
  --set-secrets "DATABASE_URL=AUTH_JWT_SECRET:latest"  # deliberately wrong secret

# The service will fail to start; Cloud Run returns 503 to callers
curl https://<accounts-url>/health
# → HTTP 503

# Check the revision status
gcloud run revisions list --service accounts-service --region us-central1

# Roll back to the previous stable revision
PREV=$(gcloud run revisions list --service accounts-service \
  --region us-central1 --format="value(name)" | sed -n '2p')
gcloud run services update-traffic accounts-service \
  --region us-central1 \
  --to-revisions "${PREV}=100"
```

### Expected behavior
- Cloud Run automatically keeps the previous revision running while the new one is unhealthy.
- Traffic is not cut over until the new revision passes its health check.
- If the current revision is already bad: use `update-traffic` to route 100% to the last known-good revision.

### Rollback SOP
1. Identify the bad revision: `gcloud run revisions list --service <svc> --region us-central1`
2. Find the last healthy revision (second in the list, or last with traffic > 0).
3. Reroute: `gcloud run services update-traffic <svc> --region us-central1 --to-revisions <rev>=100`
4. Fix the root cause, push a corrected image, let CI deploy.
5. Verify health: `curl https://<svc-url>/health`

---

## 6. Rollback Procedure

This is the canonical rollback SOP for any InfraPortal service.

### Step 1 — Identify the incident
```bash
# Check all service health endpoints
for svc in accounts contacts activities opportunities reporting search audit spend projects; do
  echo -n "$svc: "
  curl -sf "https://<${svc}-url>/health" | jq -r .status 2>/dev/null || echo "UNREACHABLE"
done
```

### Step 2 — Check recent revisions
```bash
gcloud run revisions list --service <service-name> --region us-central1 \
  --format "table(name,status.conditions[0].type,status.conditions[0].status,createTime)"
```

### Step 3 — Roll back traffic
```bash
PREV_REVISION=<revision-name-from-step-2>
gcloud run services update-traffic <service-name> \
  --region us-central1 \
  --to-revisions "${PREV_REVISION}=100"
```

### Step 4 — Verify
```bash
curl -s https://<service-url>/health | jq .
```

### Step 5 — Post-incident
- Document the incident in a GitHub Issue with: timeline, root cause, fix applied, prevention.
- Update this runbook if a new failure mode was discovered.

---

*Last updated: 2026-04-11 — v1.2.4*
