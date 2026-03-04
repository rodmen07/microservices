# Microservices Workspace Specification

This folder contains a multi-service ecosystem:

- `backend-service` (Rust/Axum API + SQLite)
- `ai-orchestrator-service` (Python planner microservice)
- `frontend-service` (React/Vite TypeScript UI)
- `auth-service` (Python JWT issuance/verification microservice)
- `accounts-service` (Rust/Axum account and tenant domain API)
- `contacts-service` (Rust/Axum contact and lead domain API)

The goal is to keep the platform easy to run locally while preserving stable cross-service contracts.

## 1) System architecture

### Service boundaries

- **frontend-service** owns web UX, stateful task interactions, and goal visualization.
- **backend-service** owns canonical task CRUD APIs, validation rules, and persistence.
- **ai-orchestrator-service** owns provider-facing AI logic and converts goals to task lists.
- **auth-service** owns token issuance/verification and centralized auth contract.

### Request flow for planning

1. Frontend sends a long-term goal to backend (`POST /api/v1/tasks/plan`).
2. Backend calls orchestrator `POST /plan` via internal HTTP.
3. Orchestrator calls provider APIs (OpenRouter-configured) and returns tasks.
4. Backend returns tasks to frontend in stable JSON.

### Core design principle

Provider details must stay isolated in `ai-orchestrator-service`; neither frontend nor backend should implement direct provider-specific planning logic.

## 2) Contract snapshot

### Orchestrator API

- `GET /health` -> `{ "status": "ok" }`
- `POST /plan`
  - Request: `{ "goal": string }`
  - Response: `{ "tasks": string[] }`

### Backend API (v1)

- `GET /health` (process liveness)
- `GET /ready` (database readiness)
- `GET /api/v1/tasks` (`limit`, `offset`, `completed`, `q`)
- `POST /api/v1/tasks`
- `POST /api/v1/tasks/plan`
- `PATCH /api/v1/tasks/{id}`
- `DELETE /api/v1/tasks/{id}`

### Auth API (MVP)

- `GET /health` -> `{ "status": "ok" }`
- `POST /auth/token`
  - Request: `{ "subject": string, "roles": string[] }`
  - Response: `{ "access_token": string, "token_type": "bearer", "expires_in": number }`
- `POST /auth/verify`
  - Request: `{ "token": string }`
  - Response: `{ "active": boolean, "subject"?: string, "roles"?: string[], "exp"?: number, "issuer"?: string }`

### Backend invariants

- `title` required (trimmed non-empty)
- `title` max length `120`
- Task list ordering stable by `id ASC`
- Non-2xx response envelope: `{ code, message, details? }`

## 3) Environment and defaults

### backend-service

- Default bind: `0.0.0.0:3000`
- Key env vars: `HOST`, `PORT`, `DATABASE_URL`, `AI_ORCHESTRATOR_PLAN_URL`
- Default planner URL: `http://127.0.0.1:8081/plan`

### ai-orchestrator-service

- Default port: `8081` (`APP_PORT`)
- Key env vars:
  - `OPENROUTER_API_KEY`
  - `OPENROUTER_MODEL` (default `google/gemma-3-4b-it:free`)
  - `OPENROUTER_BASE_URL` (default `https://openrouter.ai/api/v1`)
  - `REQUEST_TIMEOUT_SECONDS` (default `30`)

### auth-service

- Default port: `8082` (`APP_PORT`)
- Key env vars:
  - `AUTH_JWT_SECRET`
  - `AUTH_JWT_ALGORITHM` (default `HS256`)
  - `AUTH_TOKEN_EXPIRES_SECONDS` (default `3600`)
  - `AUTH_ISSUER` (default `auth-service`)

### frontend-service

- Uses `VITE_API_BASE_URL` for backend base URL
- Local default backend: `http://localhost:3000`
- Production fallback should point to deployed backend URL

## 4) Local development order

1. Start `ai-orchestrator-service` on port `8081`.
2. Start `backend-service` on port `3000` (with planner URL set if non-default).
3. Start `frontend-service` and verify planner + CRUD flows.

## 5) Quality and CI expectations

### backend-service

- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

### ai-orchestrator-service

- `pytest`
- Preserve timeout + error handling behavior for provider calls

### frontend-service

- `npm run build`
- Keep TypeScript strict-mode compatibility

## 6) Deployment notes

- Frontend is configured for GitHub Pages deployment from `dist/`.
- Frontend base path must remain compatible with `/frontend-service/` unless deployment strategy changes.
- Backend + orchestrator can be deployed independently; planner URL wiring is done through `AI_ORCHESTRATOR_PLAN_URL` in backend.

## 7) Change management guardrails

- Prefer additive changes over breaking contract edits.
- If changing API payloads, update all impacted services in one change set.
- Keep environment variable names stable unless migration guidance is added.
- Preserve Decap CMS paths in frontend unless explicitly changing CMS strategy:
  - `public/admin/config.yml`
  - `public/content/site.json`
  - `/admin/` (dev) and `/frontend-service/admin/` (Pages)

## 8) Future compatibility targets

- Authentication is not enforced in backend v1, but future interface is reserved:
  - `Authorization: Bearer <token>`
- New auth work should preserve backward compatibility or include explicit versioning.

## 9) Definition of done for cross-service changes

- Service-local tests/build succeed.
- Contracts remain consistent across frontend, backend, and orchestrator.
- README and instructions updated when behavior/config/contracts change.

## 10) CRM microservices roadmap to-do

- [x] `accounts-service` scaffolded (account/tenant domain baseline)
- [x] `contacts-service` scaffolded (contacts/leads domain baseline)
- [x] `opportunities-service` (pipeline, stages, forecasting)
- [x] `activities-service` (emails/calls/meetings/tasks timeline)
- [ ] `automation-service` (workflows/triggers/queue workers)
- [ ] `integrations-service` (email/calendar/webhook connectors)
- [ ] `search-service` (cross-entity indexing + global search)
- [ ] `reporting-service` (dashboards, exports, analytics)
