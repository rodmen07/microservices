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

`cargo` is NOT on the bash tool PATH. It is only available in the user's VS Code integrated terminal. Do NOT attempt to run `cargo build`, `cargo test`, or `cargo check` via the Bash tool — they will fail silently or with "command not found". Write correct code and let the user build. If a type error is suspected, reason through it manually rather than running the compiler.

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

**Completion states:** Planned → Implemented → Published.
Published means all Release Locations below have been updated.

---

## Release Locations

Every location that must be updated when publishing a version. This list is the canonical source for the admin release checklist at `#/admin`.

| # | Location | Path / URL |
|---|----------|------------|
| 1 | Patch Notes page | `standalones/frontend-service/src/pages/PatchNotesPage.tsx` |
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
