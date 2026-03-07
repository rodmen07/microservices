# Claude instructions — microservices workspace

## What this project is

TaskForge: a portfolio microservices system. Nine independently deployed Rust/Axum services, all production-grade with SQLite persistence and JWT auth.

**Production Rust services (workspace):**
| Service | Port | Notes |
|---------|------|-------|
| `task-api-service` | 3000 | Reference impl. AI planner proxy, admin metrics. |
| `accounts-service` | 3010 | Status tracking: active/inactive/churned. |
| `contacts-service` | 3011 | Lifecycle stages, cross-service account FK validation. |
| `opportunities-service` | 3012 | Pipeline stages: qualification → proposal → closed. |
| `activities-service` | 3013 | Activity types: call/email/meeting/task. |
| `automation-service` | 3014 | Event-driven trigger/action workflows. |
| `integrations-service` | 3015 | Third-party provider connection registry. |
| `search-service` | 3016 | Full-text search across entity types. |
| `reporting-service` | 3017 | Saved reports + dashboard summary. |

**Non-Rust (standalone repos — see `standalones/`):**
- `standalones/ai-orchestrator-service` — Python/FastAPI, internal-only, calls Anthropic Claude API.
- `standalones/auth-service` — Python/FastAPI, deployed on Fly.io. Full user auth: password + GitHub OAuth + Google OAuth + password reset.
- `standalones/backend-service` — Rust/Axum, deployed on Fly.io. Also a Rust workspace member.
- `standalones/frontend-service` — React 19 + TypeScript + Vite + Tailwind v3. Deployed to GitHub Pages.

---

## Directory layout

```
d:\Projects\microservices\
  accounts-service/       Rust workspace — production (port 3010)
  contacts-service/       Rust workspace — production (port 3011)
  opportunities-service/  Rust workspace — production (port 3012)
  activities-service/     Rust workspace — production (port 3013)
  automation-service/     Rust workspace — production (port 3014)
  integrations-service/   Rust workspace — production (port 3015)
  search-service/         Rust workspace — production (port 3016)
  reporting-service/      Rust workspace — production (port 3017)
  standalones/
    backend-service/      Rust/Axum — own git repo (remote: backend-service)
    auth-service/         Python/FastAPI — own git repo (remote: auth-service)
    ai-orchestrator-service/ Python/FastAPI — own git repo (remote: ai-orchestrator-service)
    frontend-service/     React/Vite — own git repo (remote: frontend-service)
  Cargo.toml              workspace root
  CLAUDE.md               this file
```

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
  fly.toml            # Fly.io deployment config
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
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "macros", "migrate"] }
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
pub async fn from_database_url(database_url: &str) -> Result<Self, sqlx::Error> {
    let pool = SqlitePoolOptions::new().max_connections(5).connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(Self { pool })
}
```

- IDs are `TEXT` (UUID v4 strings), never integer autoincrement for new services.
- Timestamps are `TEXT` in `"%Y-%m-%dT%H:%M:%SZ"` format via `chrono::Utc`.
- `FromRow` derive on domain model structs; SELECT column order must match struct field order.
- Default `DATABASE_URL`: `sqlite://<service-name>.db` (local) or `sqlite:///data/<service-name>.db` (Fly.io volume).

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

## Fly.io deployment

Each service needs:
- `fly.toml` with `[mounts]` pointing to `/data` (SQLite volume).
- `DATABASE_URL = "sqlite:///data/<service>.db"` in `[env]`.
- `PORT = "8080"` in `[env]` (Fly uses 8080 internally).
- Health check at `/health` returning `{ "status": "ok" }`.

Secrets set via `fly secrets set AUTH_JWT_SECRET=... ALLOWED_ORIGINS=...`.

---

## Frontend (standalones/frontend-service)

Separate git repo. Located at `d:\Projects\microservices\standalones\frontend-service\`.

- React 19 + TypeScript + Vite + Tailwind v3
- Hash-based router: `window.location.hash` + `hashchange` event in `src/main.tsx`
- To add a page: create `src/pages/MyPage.tsx`, import in `main.tsx`, add `if (hash === '#/mypage') return <MyPage />`
- CMS-driven content via JSON files in `public/content/` fetched at runtime
- `src/api/tasks.ts` — all API calls; `src/types.ts` — shared TypeScript types
- Kanban board has HTML5 DnD already implemented
- No routing library (react-router-dom) — intentional, hash router is ~15 lines

---

## Git

Five repos total:
- `d:\Projects\microservices\` — Rust workspace root (remote: `microservices`)
- `d:\Projects\microservices\standalones\backend-service\` — (remote: `backend-service`)
- `d:\Projects\microservices\standalones\auth-service\` — (remote: `auth-service`)
- `d:\Projects\microservices\standalones\ai-orchestrator-service\` — (remote: `ai-orchestrator-service`)
- `d:\Projects\microservices\standalones\frontend-service\` — (remote: `frontend-service`)

Commit standalone repos separately. The root microservices repo tracks only the workspace-only Rust stubs.

---

## Upgrade checklist for a stub service

When upgrading any of the remaining stubs, do in order:
1. Update `Cargo.toml` — add sqlx, jsonwebtoken, chrono; upgrade axum to 0.8, tower-http to 0.6
2. Add `[lib]` + `[[bin]]` sections to `Cargo.toml`
3. Create `migrations/0001_create_<table>.sql`
4. Create `src/lib/` directory structure
5. Write `models.rs`, `auth.rs` (copy verbatim), `app_state.rs`, `handlers/`, `router.rs`
6. Rewrite `src/lib.rs` with `#[path]` declarations
7. Rewrite `src/main.rs` to use `AppState::from_database_url` + `build_router`
8. Add `fly.toml`
9. Update `standalones/frontend-service/public/content/roadmap.json` to mark as shipped
