# Claude instructions — microservices workspace

## What this project is

InfraPortal: a portfolio microservices system. Ten independently deployed Rust services, all production-grade with Cloud SQL PostgreSQL persistence and JWT authentication.

**Deployed and production-grade:**
- `task-api-service` — Rust/Axum, PostgreSQL, JWT auth, AI planner proxy. Port 3000. The reference implementation.
- `accounts-service` — Rust/Axum, PostgreSQL, JWT auth. Port 3010.
- `contacts-service` — Rust/Axum, PostgreSQL, JWT auth, cross-service account validation. Port 3011.
- `activities-service` — Rust/Axum, PostgreSQL, JWT auth. Port 3013.
- `automation-service` — Rust/Axum, PostgreSQL, JWT auth.
- `integrations-service` — Rust/Axum, PostgreSQL, JWT auth.
- `opportunities-service` — Rust/Axum, PostgreSQL, JWT auth.
- `reporting-service` — Rust/Axum, PostgreSQL, JWT auth, saved report CRUD, /dashboard.
- `search-service` — Rust/Axum, PostgreSQL, JWT auth, write-through indexing.

**Non-Rust:**
- `ai-orchestrator-service` — Python/FastAPI, internal-only, calls Anthropic Claude API.
- `auth-service` — minimal implementation.

**Frontend:** `frontend-service` (React 19 + TypeScript + Vite + Tailwind v3) lives in a separate repo at `d:\Projects\microservices\frontend-service\` but is git-tracked separately (remote: `frontend-service`).

---

## Shell / build environment

`cargo` is NOT available in the Windows bash shell due to missing `dlltool.exe`. Do NOT attempt to run `cargo build`, `cargo test`, or `cargo check` via the Bash tool — they will fail with "error calling dlltool 'dlltool.exe': program not found". Write correct code and let the user build.

**Workaround:** `cargo` IS available in the VS Code integrated terminal using WSL (Ubuntu 22.04). The user can open the integrated terminal (Ctrl+`) and run commands there. If a type error is suspected, reason through it manually rather than running the compiler.

The workspace has a GitHub Actions CI pipeline (`.github/workflows/rust.yml`) that runs `cargo build` and `cargo test` on push.

---

## Rust service architecture — standard pattern

All production Rust services follow the `task-api-service` layout exactly. When upgrading a stub, replicate this structure:

```
<service>/
  Cargo.toml          # see dependency versions below
  migrations/
    0001_create_<table>.sql
  src/
    main.rs           # entrypoint only: read env vars, init AppState, bind listener
    lib.rs            # #[path] declarations + pub use re-exports
    lib/
      app_state.rs    # SqlitePool (+ reqwest::Client if cross-service calls needed)
      auth.rs         # JWT validation (copy from accounts-service verbatim)
      models.rs       # all structs: domain model, request/response DTOs, ApiError, HealthResponse
      router.rs       # build_router(), build_cors_layer()
      handlers/
        mod.rs        # pub mod declarations + pub(crate) use re-exports
        health.rs     # health() handler
        <resource>.rs # CRUD handlers
```

### lib.rs must use #[path] attributes

```rust
#[path = "lib/app_state.rs"]
pub mod app_state;
#[path = "lib/auth.rs"]
pub mod auth;
#[path = "lib/handlers/mod.rs"]
pub mod handlers;
#[path = "lib/models.rs"]
pub mod models;
#[path = "lib/router.rs"]
pub mod router;

pub use app_state::AppState;
pub use router::build_router;
```

Without `#[path]`, Rust looks for `src/app_state.rs` not `src/lib/app_state.rs`.

---

## Dependency versions (use these, not older ones)

```toml
axum = { version = "0.8", features = ["macros"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "macros", "migrate"] }
jsonwebtoken = "8.3.0"
chrono = { version = "0.4", features = ["clock"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

---

## axum 0.8 specifics

- Path params use **`{id}`** not `:id`: `.route("/api/v1/things/{id}", ...)`
- `Path<String>` extractor works with `{id}` routes.
- Middleware uses `from_fn` / `from_fn_with_state` from `axum::middleware`.
- `Request` is `axum::extract::Request` (not `http::Request<Body>`).

---

## Auth pattern

All protected endpoints validate JWT in the handler directly (not middleware), via:

```rust
fn require_auth(headers: &HeaderMap) -> Result<(), Response> {
    let header_value = headers.get("Authorization").and_then(|v| v.to_str().ok());
    validate_authorization_header(header_value)
        .map(|_| ())
        .map_err(|err| error_response(StatusCode::UNAUTHORIZED, err.code(), err.message()))
}
```

The `auth.rs` module is identical across all services. Key env vars:
- `AUTH_JWT_SECRET` (default: `"dev-insecure-secret-change-me"`)
- `AUTH_JWT_ALGORITHM` (default: `HS256`; supports RS256/RS384/RS512/HS384/HS512)
- `AUTH_ISSUER` (default: `"auth-service"`)

---

## Error envelope

All errors must return `{ code, message, details? }`:

```rust
#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
```

Use `StatusCode` constants (`BAD_REQUEST`, `NOT_FOUND`, `UNPROCESSABLE_ENTITY`, etc.) — never raw numbers.

---

## Database pattern

```rust
// app_state.rs
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
    let pool = PgPoolOptions::new().max_connections(5).connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(Self { pool })
}
```

- IDs are `TEXT` (UUID v4 strings), never integer autoincrement for new services.
- Timestamps are `TEXT` in `"%Y-%m-%dT%H:%M:%SZ"` format via `chrono::Utc`.
- `FromRow` derive on domain model structs; SELECT column order must match struct field order.
- SQL placeholders are numbered (`$1`, `$2`, ...) — not `?` (SQLite syntax).
- Dynamic WHERE queries use a `param_idx: usize` counter to generate correct `$N` placeholders.
- `INSERT ... ON CONFLICT DO NOTHING` for upsert/dedup (not `INSERT OR IGNORE`).
- Migration timestamps: `DEFAULT (to_char(timezone('UTC', now()), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))`.
- Default `DATABASE_URL`: `postgres://postgres:postgres@localhost:5432/<service-name>` (local).
- Cloud SQL (production): `postgres://user:pass@/<dbname>?host=/cloudsql/PROJECT:REGION:INSTANCE`.

---

## Cross-service calls

When a service needs to validate a foreign key from another service (e.g. contacts → accounts):
1. Add `reqwest::Client` to `AppState`.
2. Read `ACCOUNTS_SERVICE_URL` (or equivalent) from env.
3. **Fail-open** if the env var is not set (local dev without all services running).
4. Pass the caller's Bearer token through to the upstream service.

---

## CORS

`ALLOWED_ORIGINS` env var — comma-separated list of allowed origins. Empty = no cross-origin allowed. `*` = fully permissive (warn + use `CorsLayer::permissive()`).

---

## Google Cloud deployment

CI/CD deploys Rust services to Google Cloud Run and pushes images to Artifact Registry.
Persistence: Cloud SQL PostgreSQL 16 instance at `microservices-489413:us-south1:microservices-pg`.
Each service connects to its own database on the shared instance via Unix socket.

Expected GitHub configuration:
- Repository variable `GCP_PROJECT_ID`.
- Repository variable `ALLOWED_ORIGINS`.
- Repository secret `GCP_WORKLOAD_IDENTITY_PROVIDER` (WIF provider resource name).
- Repository secret `GCP_SERVICE_ACCOUNT` (deployer SA email).
- Repository secret `AUTH_JWT_SECRET`.
- Per-service Secret Manager secrets: `ACCOUNTS_DB_URL`, `CONTACTS_DB_URL`, `ACTIVITIES_DB_URL`,
  `AUTOMATION_DB_URL`, `INTEGRATIONS_DB_URL`, `OPPORTUNITIES_DB_URL`, `REPORTING_DB_URL`,
  `SEARCH_DB_URL`, `SPEND_DB_URL`, `PROJECTS_DB_URL`.
  Format: `postgres://appuser:pass@/<dbname>?host=/cloudsql/microservices-489413:us-south1:microservices-pg`

The deployer SA needs roles: Artifact Registry Writer, Cloud Run Developer, Cloud SQL Client,
Secret Manager Secret Accessor.

Runtime configuration (service-level):
- `PORT` is injected by Cloud Run.
- `DATABASE_URL` comes from per-service Secret Manager secret (see above).
- `AUTH_JWT_SECRET` and `ALLOWED_ORIGINS` come from secrets/variables.
- Health check endpoint remains `/health` returning `{ "status": "ok" }`.

---

## Frontend (frontend-service)

Separate git repo. Located at `d:\Projects\microservices\frontend-service\`.

- React 19 + TypeScript + Vite + Tailwind v3
- Hash-based router: `window.location.hash` + `hashchange` event in `src/main.tsx`
- To add a page: create `src/pages/MyPage.tsx`, import in `main.tsx`, add `if (hash === '#/mypage') return <MyPage />`
- CMS-driven content via JSON files in `public/content/` fetched at runtime

---

## Git

- `d:\Projects\microservices\` — Rust workspace (remote: `microservices`)
- `d:\Projects\microservices\frontend-service\` — React frontend (remote: `frontend-service`)
- Commit both repos separately when making cross-cutting changes.

---

## Roadmap

### v0.4 — Language Breadth & AI Depth ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v0.4.1 | AI Consulting Feature | Published (2026-03-17) |
| v0.4.2 | Django REST API (`observaboard`) | Published (2026-03-17) |
| v0.4.3 | Go Service | Published (2026-03-17) |
| v0.4.4 | Frontend UI Expansion (CRM CRUD, Live Feed, Search, Reports, Observaboard pages) | Published (2026-03-19) |

### v0.5 — Platform Completeness ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v0.5.1 | reporting-service production upgrade (PostgreSQL, JWT auth, saved report CRUD, /dashboard) | Published (2026-03-23) |
| v0.5.2 | search-service write-through indexing (upsert/delete from CRM services, retry logic) | Published (2026-03-23) |
| v0.5.3 | activities-service production upgrade (PostgreSQL, JWT auth, CRUD) | Published (2026-03-23) |
| v0.5.4 | automation-service production upgrade (PostgreSQL, JWT auth, workflow rules) | Published (2026-03-23) |
| v0.5.5 | integrations-service production upgrade (PostgreSQL, JWT auth, connection registry) | Published (2026-03-23) |
| v0.5.6 | opportunities-service production upgrade (PostgreSQL, JWT auth, stage tracking) | Published (2026-03-23) |

### v1.0 — Client Portal ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v1.0.1 | projects-service — Rust/Axum client portal API (projects, milestones, deliverables) | Published (2026-03-29) |
| v1.0.2 | go-gateway — Go API gateway deployed to GCP Cloud Run | Published (2026-03-29) |
| v1.0.3 | GCP Cloud Run migration — 11 services (OIDC + WIF, Artifact Registry, Secret Manager) | Published (2026-03-29) |
| v1.0.4 | OAuth flows — GitHub + Google client portal sign-in with client-role JWT | Published (2026-03-29) |
| v1.0.5 | Admin provisioning UI — create projects, milestones, deliverables; assign to client users | Published (2026-03-29) |

### v1.1 — Developer Experience & Portfolio Quality ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v1.1 | CI/CD pipeline — two-stage runner image build/test across full workspace | Published (2026-04-09) |
| v1.1.1 | Gemini API — `/consult/gemini` + stream endpoints in ai-orchestrator; Claude/Gemini toggle in frontend | Published (2026-04-10) |
| v1.1.2 | Portfolio narrative fixes — Dockerfiles cleaned of SQLite; docs corrected to PostgreSQL (Cloud SQL) | Published (2026-04-10) |
| v1.1.3 | activities-service cross-service validation — account_id/contact_id validated on create; Terraform extra_env wiring | Published (2026-04-10) |

### v1.2 — Operational Maturity ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v1.2.1 | Data export pipeline — bulk CSV/JSON from reporting-service; admin export modal | Published (2026-04-11) |
| v1.2.2 | Audit trail — new audit-service (Rust), immutable CRM mutation log, admin audit page | Published (2026-04-11) |
| v1.2.3 | Portfolio observability — CRM events → Observaboard; admin service health dashboard | Published (2026-04-11) |
| v1.2.4 | Service resilience — E2E test suite, load testing, chaos engineering runbook | Published (2026-04-11) |

### v1.3 — Autonomous Operations ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|-----------------|
| v1.3.1 | Productionizer agent — Gemini 2.5 Flash autonomous coding agent; daily GitHub Actions cron; PRs to microservices repo | Published (2026-04-15) |
| v1.3.2 | Client Portal Dashboard — full-featured project tracking portal for clients; OAuth/email auth, milestones + deliverables, effort tracking (hours), project links, Gmail-synced emails, progress updates, messaging, GitHub build status | Published (2026-06-01) |

### v1.4 — Cloud Consolidation ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.4.0 | Fly.io to GCP Cloud Run migration (ai-orchestrator-service, event-stream-service); keyless OIDC; port normalisation; SHA-pinned images; Cloud Migration case study | Published (2026-05-07) |

### v1.5 — DB Migration & Live Events ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.5.0 | backend-service: SQLite (Fly.io) to PostgreSQL (GCP Cloud Run + Cloud SQL); sqlx postgres, $N placeholders, RETURNING, BIGSERIAL/BOOLEAN/TIMESTAMPTZ migrations; CRM notification bell (SSE EventSource, auto-reconnect, unread badge, dropdown panel) | Published (2026-05-08) |

### v1.6 — Observability & Compliance ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.6.0 | observaboard: Fly.io to Cloud Run (remove Celery/Redis, inline classification, port 8080, deploy-cloud-run.yml with migrate job); SOC 2 CC9.2 Terraform (vendor inventory labels, Cloud Run attestation, 5xx alert); portfolio polish (MicroservicesCaseStudyPage tech stack + baseline note) | Published (2026-05-08) |

### v1.7 - CRM Event Pipeline - Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.7.0 | go-gateway: internal/observer package intercepts 2xx CRM mutations and fires fire-and-forget ingest events to observaboard; proxy.New() gains optional observer param; config gains ObservaboardURL/ObservaboardAPIKey; deploy workflow adds OBSERVABOARD_URL var + OBSERVABOARD_API_KEY secret; observaboard: create_gateway_api_key management command (idempotent, prints raw key once); infraportal: SOURCE_COLORS map for source-driven notification badge colors | Published (2026-05-07) |

### v1.8 - Real-Time Feedback Loop - Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.8.0 | observaboard: stream_publisher.py fires classified events to event-stream-service via short-lived HS256 JWT (stdlib hmac, 2s timeout, all exceptions swallowed); IngestView.post wired with refresh_from_db + publish_to_stream guard; settings.py adds EVENT_STREAM_URL/EVENT_STREAM_JWT_SECRET; deploy-cloud-run.yml resolves event-stream URL + injects JWT secret; create_gateway_api_key bug fixed (create_key -> create, prefix -> pk); NotificationBell badge split fix (type.split('.')[0]) | Published (2026-05-07) |

### v1.9 - Distributed Tracing & Observability - Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.9.0 | OpenTelemetry integration: W3C traceparent middleware in go-gateway; Cloud Trace exporter + graceful fallback in all 11 Rust services + ai-orchestrator (Python); event-stream-service traceparent extraction; rustls-webpki security upgrade (RUSTSEC-2026-0104) enforced via workspace.dependencies; end-to-end request tracing from gateway through all services visible in GCP Cloud Trace | Published (2026-05-07) |

### v1.10 — Gateway Rate Limiting ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.10.0 | Per-client IP rate limiting with route-tier overrides (auth 5 rps, write 30 rps, read 60 rps); X-RateLimit-* response headers; configurable burst and idle eviction; 9 unit tests covering headers, 429 enforcement, client isolation, and X-Forwarded-For handling | Published (2026-05-16) |

### v1.11 — Multi-Region HA & Event-Driven Batch ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.11.1 | Multi-region Cloud SQL setup: primary (us-central1) with read replica (us-east1); Cloud SQL Auth proxy for Kubernetes-style sidecar pattern; sqlx with replica routing hints | Published (2026-05-16) |
| v1.11.2 | Batch mutation events from CRM services → Pub/Sub topic for downstream processing; audit-service publishes events; observaboard subscribes for classification and fan-out | Published (2026-05-16) |
| v1.11.3 | Cloud Tasks async job queue for long-running workloads; Terraform module for queue provisioning; integration with ai-orchestrator for scheduled consulting reports | Published (2026-05-16) |

### v1.12 — IaC Root Module, JWT Auth & CI/CD ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.12.1 | Terraform root module (terraform/envs/prod) for multi-service orchestration; environment-specific variables for region, project, service replicas, and secrets injection | Published (2026-05-16) |
| v1.12.2 | JWT auth hardened: token rotation every 24h, refresh endpoint in auth-service, client-side refresh middleware in all services | Published (2026-05-16) |
| v1.12.3 | CI/CD multi-stage: build runner image → test all services → push to Artifact Registry → deploy to Cloud Run with canary validation | Published (2026-05-16) |
| v1.12.4 | Service resilience: integration tests for all 11 services, k6 load/spike/smoke scenarios, chaos engineering runbook (cold-start, connection exhaustion, fail-open paths) | Published (2026-05-16) |

### v1.13 — Production Hardening, IaC Completeness & Observaboard Depth ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.13.1 | Cloud Armor DDoS protection for go-gateway; geo-restriction rules (US only by default); rate limiting at ingress layer | Published (2026-05-16) |
| v1.13.2 | VPC Service Controls: private Cloud SQL, Cloud Run only accessible via Private Service Connection; egress filtering | Published (2026-05-16) |
| v1.13.3 | Cloud SQL automatic backups with 30-day retention; point-in-time recovery testing and runbook | Published (2026-05-16) |
| v1.13.4 | observaboard: Django signals for CRM event classification, severity scoring, and automated alerting on high-severity events | Published (2026-05-16) |
| v1.13.5 | Observaboard webhooks: HTTP POST to external systems on new high-severity events; configurable retry and timeout | Published (2026-05-16) |
| v1.13.6 | Cloud Logging sink: all service logs shipped to BigQuery for long-term analysis; saved queries for per-service error rates | Published (2026-05-16) |
| v1.13.7 | Secret rotation: auth-service JWT secret rotates every 7 days; new versions staged before cutover; clients refresh on 401 | Published (2026-05-16) |
| v1.13.8 | Terraform: Cloud KMS module for customer-managed encryption keys; Cloud SQL, Cloud Storage, and Secrets encrypted with project-owned keys | Published (2026-05-16) |
| v1.13.9 | Cost optimization: committed-use discounts for Cloud Run and Cloud SQL; resource requests/limits tuned per workload via Terraform | Published (2026-05-16) |

### v1.14 — Security Depth, Cost Efficiency & E2E Quality ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.14.1 | Cloud SQL proxy encryption in-transit: REQUIRE SSL enforced at database level; cert pinning in client connections | Published (2026-05-16) |
| v1.14.2 | Service-to-service auth: internal Cloud Run services authenticate via service account impersonation and short-lived tokens | Published (2026-05-16) |
| v1.14.3 | OAuth client secret rotation: GitHub + Google credentials cycled monthly via Terraform and Secret Manager | Published (2026-05-16) |
| v1.14.4 | E2E test suite: Playwright flows for auth, portal, CRM admin, reports — all green before release | Published (2026-05-16) |
| v1.14.5 | Infracost integration in deploy workflow: cost estimate posted to PRs; prevents surprise bill shocks from infrastructure changes | Published (2026-05-16) |
| v1.14.6 | BigQuery daily aggregates: scheduled query for CRM mutation summaries (per-day, per-resource-type, per-method counts) | Published (2026-05-16) |
| v1.14.7 | Compute cost analysis: per-service CPU and memory utilization tracked; rightsizing recommendations from GCP Console | Published (2026-05-16) |
| v1.14.8 | Network cost optimization: Cloud CDN for static assets; Cloud Interconnect evaluated for hybrid setups | Published (2026-05-16) |
| v1.14.9 | Terraform: saved cost analysis and commit-to-cost validation in CI; deployment blocked if delta > threshold | Published (2026-05-16) |
| v1.14.10 | Budget alerts: GCP billing alerts configured in Terraform; email notification on 50%, 90%, 100% of monthly spend cap | Published (2026-05-16) |
| v1.14.11 | Waste reduction: auto-deletion of old Cloud Run revisions after 30 days (retain only active); Cloud Storage lifecycle policies for old logs | Published (2026-05-16) |
| v1.14.12 | v1.14 Patch Notes, README & Final Commit: documentation across all release locations | Published (2026-05-16) |

### v1.15 — Deployment Safety, SLO Monitoring & Distributed State ✅ Complete

| Sub-version | Feature | Completion State |
|-------------|---------|------------------|
| v1.15.1 | Cloud Run canary deployments: no-traffic revision → smoke test → 10/90 traffic split → full promotion; auto-rollback on failure | Published (2026-05-17) |
| v1.15.2 | Smoke-test script (scripts/smoke-test.sh): validates /health payload and /health/upstreams reachability with configurable retries | Published (2026-05-17) |
| v1.15.3 | Automated rollback composite action (.github/actions/cloud-run-rollback): reusable GitHub action for instant rollback on canary failure | Published (2026-05-17) |
| v1.15.4 | SLO Terraform module (terraform/slos): availability SLO (99.9% target), latency SLO (< 2s p99); environment-configurable thresholds | Published (2026-05-17) |
| v1.15.5 | SLO burn-rate alerts: fast-burn (high error budget consumption) and slow-burn (sustained degradation) alert policies in Cloud Monitoring | Published (2026-05-17) |
| v1.15.6 | Per-service uptime checks: 60-second cadence, 3-minute sustained-failure detection window; alert policies per service | Published (2026-05-17) |
| v1.15.7 | Redis Terraform module (terraform/memorystore): provision Memorystore Redis for gateway distributed state; optional Secret Manager integration | Published (2026-05-17) |
| v1.15.8 | Redis-backed distributed rate limiting: INCR + EXPIRE fixed windows shared across all Cloud Run instances; fail-open on Redis unavailability | Published (2026-05-17) |
| v1.15.9 | Gateway response cache: short-TTL in-process LRU for read endpoints; X-Cache MISS/HIT headers for visibility; per-user cache isolation | Published (2026-05-17) |
| v1.15.10 | v1.15 Patch Notes, README & Final Commit: documentation wrap-up for deployment safety and SLO monitoring | Published (2026-05-17) |

**Completion states:** Planned → Implemented → Published.
Published means all Release Locations below have been updated.

---

## Release Locations

Every location that must be updated when publishing a version. This list is the canonical source for the admin release checklist at `#/admin`.

| # | Location | Path / URL |
|---|----------|------------|
| 1 | Patch Notes page | `infraportal/src/pages/PatchNotesPage.tsx` |
| 2 | Portfolio README | `README.md` (root of Portfolio repo) |
| 3 | CLAUDE.md instructions | `microservices/CLAUDE.md` (this file) — update Roadmap table |
| 4 | Memory — next session todos | `C:\Users\rodme\.claude\projects\d--Projects\memory\project_next_session_todos.md` |
| 5 | MEMORY.md index | `C:\Users\rodme\.claude\projects\d--Projects\memory\MEMORY.md` |
| 6 | GitHub release tag | https://github.com/rodmen07/portfolio/releases — create release for tag |

---

## Service upgrade history

All nine Rust services have been upgraded from stubs to production-grade (PostgreSQL via Cloud SQL, JWT auth, full CRUD). The upgrade followed this standard checklist:
1. Update `Cargo.toml` — add sqlx, jsonwebtoken, chrono; upgrade axum to 0.8, tower-http to 0.6
2. Add `[lib]` + `[[bin]]` sections to `Cargo.toml`
3. Create `migrations/0001_create_<table>.sql`
4. Create `src/lib/` directory structure
5. Write `models.rs`, `auth.rs` (copy verbatim), `app_state.rs`, `handlers/`, `router.rs`
6. Rewrite `src/lib.rs` with `#[path]` declarations
7. Rewrite `src/main.rs` to use `AppState::from_database_url` + `build_router`
8. Add or update Cloud Run/Terraform service configuration
