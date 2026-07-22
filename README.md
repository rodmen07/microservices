# Microservices Workspace

**Live site:** https://rodmen07.github.io/infraportal/

A portfolio microservices platform: an 11-service CRM backend, each service an
independently deployed Rust/Axum API on **PostgreSQL (Cloud SQL)** with JWT auth,
shipped to **Google Cloud Run** via GitHub Actions (OIDC/WIF). This repository is
the Rust Cargo workspace; the gateway, auth, AI, event, observability, and
frontend components live in their own repos (see [Related repositories](#related-repositories)).

## Services in this workspace

All are Rust/Axum 0.8, PostgreSQL via `sqlx`, JWT-authenticated, and follow the
[standard service pattern](#service-architecture). Ports below are the local
defaults; in production `PORT` is injected by Cloud Run.

| Service | Domain | Notable endpoints |
|---|---|---|
| `accounts-service` | Accounts and tenants | account/tenant CRUD (port 3010) |
| `contacts-service` | Contacts and leads; validates `account_id` cross-service | contact CRUD (port 3011) |
| `activities-service` | Activity records; validates `account_id`/`contact_id` cross-service | activity CRUD (port 3013) |
| `opportunities-service` | Sales opportunities and pipeline stages | opportunity CRUD |
| `automation-service` | Workflow automation rules | rule CRUD |
| `integrations-service` | External-integration connection registry | connection CRUD |
| `reporting-service` | Saved reports and dashboards | report CRUD, `/dashboard` |
| `search-service` | Cross-service write-through search index | upsert/delete/query |
| `audit-service` | Immutable audit-event trail of entity mutations across the platform | `/api/v1/audit-events` (`audit_events`: entity_type, entity_id, action, actor_id, payload) |
| `projects-service` | Client-portal backend: client projects and project links | `/api/v1/projects`, `/api/v1/links/{id}` (port 3014) |
| `spend-service` | Cloud-spend tracking with per-provider ingestion | `/api/v1/spend`, `/api/v1/spend/summary`, `/api/v1/spend/sync/{gcp,flyio,github,aws}` (port 3020) |

Every service also exposes `GET /health` (process liveness, `{ "status": "ok" }`)
and `GET /ready` (database readiness).

The canonical, up-to-date architecture reference and full version history live in
[`CLAUDE.md`](CLAUDE.md) and [`ROADMAP.md`](ROADMAP.md).

## Service architecture

All services follow one layout (the `accounts-service` structure is the reference):

```
<service>/
  Cargo.toml
  migrations/
    0001_create_<table>.sql       # PostgreSQL DDL
  src/
    main.rs                       # entrypoint: read env, init AppState, bind listener
    lib.rs                        # #[path] module declarations + re-exports
    lib/
      app_state.rs                # PgPool (+ reqwest::Client for cross-service calls)
      auth.rs                     # JWT validation (identical across services)
      models.rs                   # domain model, request/response DTOs, ApiError
      router.rs                   # build_router(), build_cors_layer()
      handlers/
        mod.rs
        health.rs
        <resource>.rs             # CRUD handlers
```

Shared conventions:

- **Persistence:** PostgreSQL via `sqlx` 0.8 (`$N` placeholders, `ON CONFLICT` upserts,
  `sqlx::migrate!` on startup). IDs are UUID-v4 `TEXT`; timestamps are ISO-8601 `TEXT`.
- **Auth:** every protected handler validates a Bearer JWT via the shared `auth.rs`
  (`AUTH_JWT_SECRET`, `AUTH_JWT_ALGORITHM` default `HS256`, `AUTH_ISSUER`).
- **Errors:** a single envelope `{ code, message, details? }`, using `StatusCode` constants.
- **Cross-service validation:** a service that references another's entity (e.g. contacts →
  accounts) calls the upstream over HTTP with the caller's Bearer token, and **fails open**
  when the upstream URL env var is unset (local dev without every service running).
- **Tracing:** OpenTelemetry with a Cloud Trace exporter and graceful fallback, so a request
  is traceable across services.

See [`CLAUDE.md`](CLAUDE.md) for the exact dependency versions, axum 0.8 specifics, and the
per-service upgrade checklist.

## Local development

`cargo` runs from the workspace root against all members:

```bash
cargo build
cargo test
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

Each service defaults `DATABASE_URL` to
`postgres://postgres:postgres@localhost:5432/<service-name>` locally. Start only the
services a flow needs; cross-service validation fails open when an upstream URL is unset.

## Deployment

CI/CD (`.github/workflows/rust.yml`) builds and tests the workspace on every push and PR,
then deploys the Rust services to **Google Cloud Run** and pushes images to **Artifact
Registry**. Coverage runs on `main` only, in parallel with lint/test, to keep PR feedback fast.

- **Persistence:** Cloud SQL PostgreSQL 16 (`microservices-489413:us-south1:microservices-pg`);
  each service connects to its own database on the shared instance over a Unix socket.
- **Auth to GCP:** keyless via Workload Identity Federation (OIDC) — no long-lived credentials.
- **Per-service config:** `DATABASE_URL` from a per-service Secret Manager secret
  (`ACCOUNTS_DB_URL`, `CONTACTS_DB_URL`, …); `AUTH_JWT_SECRET` and `ALLOWED_ORIGINS` from
  secrets/variables; `PORT` injected by Cloud Run.

## Related repositories

These components deploy alongside the workspace but are **separate git repositories**, not
members of this Cargo workspace:

| Component | Stack | Role |
|---|---|---|
| `infraportal` | React 19 + Vite + TypeScript | Portfolio site and CRM/portal frontend (GitHub Pages) |
| `go-gateway` | Go | API gateway: routing, rate limiting, tracing, CRM-event observation |
| `auth-service` | Python/FastAPI | JWT issuance and verification |
| `ai-orchestrator-service` | Python/FastAPI | AI planning/consulting proxy to the Anthropic Claude API |
| `event-stream-service` | Go | Server-sent-events hub for live notifications |
| `observaboard` | Django REST | CRM-event classification, alerting, and observability |
| `task-api-service` | Rust/Axum | Original task API (the platform's first service) |

## Change-management guardrails

- Prefer additive changes over breaking contract edits; if a payload changes, update every
  impacted service in one change set.
- Keep environment-variable names stable unless migration guidance ships with the change.
- A cross-service change is done when: service-local `cargo test`/`build` succeed, contracts
  stay consistent across the services involved, and this README plus [`CLAUDE.md`](CLAUDE.md)/
  [`ROADMAP.md`](ROADMAP.md) are updated when behavior, config, or contracts change.
