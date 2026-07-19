# InfraPortal Platform Roadmap

Canonical forward-looking roadmap for the InfraPortal platform (the microservices Rust workspace plus the infraportal frontend). Created 2026-07-18.

- Historical release tables (v0.4 through v1.15.10) live in `CLAUDE.md` in this repo and are not repeated here.
- Frontend-only milestone details live in `d:/Projects/Portfolio/infraportal/ROADMAP.md`.
- The autonomous dev workflow should treat this file as the source of truth for what to pick up next, and must only pick items marked "doable now".

---

## Infrastructure status: DECOMMISSIONED (2026-06-04)

On 2026-06-04 all runtime infrastructure was decommissioned to true zero:

- Cloud SQL instance deleted. All runtime data is permanently gone (no backups).
- Both Artifact Registry repos deleted.
- All Fly.io machines and volumes destroyed.
- Every Cloud Run and Fly endpoint is offline.

Consequences:

- Nothing may be deployed and no live endpoint can be exercised.
- The GitHub Actions OIDC deploy pipelines in this repo are inert (nothing to deploy to).
- The chaos runbook (`docs/chaos-runbook.md`), the v1.15 canary/rollback machinery, and the v1.15 SLO monitoring cannot currently be exercised. They remain valid as reference material.
- The verification bar for all platform work is build + tests + CI. Never verify against a live endpoint.
- The only surface that still deploys is the infraportal frontend, via GitHub Pages on merge to main.
- Rebuilding infra would mean recreating the database and redeploying from source, not restarting anything. It is an explicit USER-ONLY decision that has not been made.

This section is the first committed record of the decommission. Older docs in this repo (the `CLAUDE.md` architecture prose, the `README.md` deployment notes, `docs/chaos-runbook.md`) still describe live infrastructure; read those passages as historical.

---

## Current state (2026-07-18)

- Last versioned release: **v1.15.10**, published 2026-05-17. No version has shipped since, although real work has landed (below).
- The workspace is 11 Rust service crates (accounts, activities, audit, automation, contacts, integrations, opportunities, projects, reporting, search, spend) plus the `shared-auth` crate. All services are axum 0.8 + sqlx 0.8 on PostgreSQL.
- Shipped since v1.15.10, currently unversioned (to be wrapped retroactively into v1.16.0):
  - 2026-06-05 (46d7a46, d8d9fff, 6d77833): adopted the published `axum-api-kit` crate as a workspace dependency; introduced the new `shared-auth` crate centralizing JWT decoder construction; migrated reporting, search, and accounts to it; cleaned up `-D warnings` CI failures across 9 services.
  - 2026-06-07 (5c4faff): fixed the `shared-auth` JWT decoder panic when `AUTH_AUDIENCE` is unset (aud is now optional unless configured).
  - 2026-07-18 (757b56e): migrated the remaining 8 services (activities, audit, automation, contacts, integrations, opportunities, projects, spend) to the `shared-auth` decoder, fixing a boot panic; net -312 lines.
  - 2026-07-18 (f8f0d0c): role-based authorization gating on spend-service and search-service routes, with new integration tests. This starts an authz-uniformity direction that v1.16.0 finishes.
- Frontend (infraportal repo): shipped an API docs page with Swagger UI on 2026-06-01 (labeled "v1.16.1" under the old private plan numbering), then pivoted to a consulting monetization funnel (2026-06-23 to 2026-06-26), then a security/truth remediation pass on 2026-07-18. Details in `d:/Projects/Portfolio/infraportal/ROADMAP.md`.

---

## How work is partitioned

Every item on this roadmap falls into exactly one bucket:

- **Doable now**: code, docs, CI, and frontend work against a mocked API layer. Verified by build + tests + CI. An autonomous agent can pick these up.
- **BLOCKED**: cannot proceed, with the blocking reason stated (usually the 2026-06-04 decommission).
- **USER-ONLY**: releases and tags, publishes, paid-account actions, and the infra-rebuild decision itself.

---

## Next milestones

STATUS 2026-07-19: every v1.16 milestone below is DELIVERED (v1.16.0 via PRs #89/#92, v1.16.1 via #93/#94, v1.16.2 via #95 plus go-gateway #10, v1.16.3 via #96/#97, v1.16.4 via infraportal #20/#21, v1.16.5 PR1 via infraportal #22), far ahead of the weekly cadence originally targeted. The sections are retained below as the delivered scope of record; the next theme (v1.17) is proposed in docs/design/V1_17_THEME.md and gated on user decisions D1-D5.

Cadence target: roughly one minor version per week, restarting at v1.16.0 the week of 2026-07-20. Each milestone is sized for one or two small PRs so the one-increment-per-run workflow can deliver it. Order below is the intended ship order; v1.16.2's rate-limit guide is independent and may land earlier if sequencing demands.

### v1.16.0 - Auth Hardening Wrap-up and Roadmap Reset (doable now)

Gives the shipped-but-unversioned June and July work a version number, finishes the authz pattern it started, and lands this roadmap as the first committed record of the decommission. Restarts the weekly cadence.

- PR 1: this `ROADMAP.md` (decommission status, doable-now vs infra-rebuild vs blocked partition, retroactive v1.16.0 notes for the shared-auth / axum-api-kit / authz work) plus a one-line pointer in `CLAUDE.md` near its Roadmap section. Written to the working tree 2026-07-18; commit pending.
- PR 2: extend the role-check gating pattern from spend-service and search-service (commit f8f0d0c) to the remaining workspace services, with integration tests per service.
- Done when: all workspace services enforce role-based authz consistently, integration tests cover the gating, and CI is green. CI green is the entire verification bar; no live endpoint exists to check.

### v1.16.1 - OpenAPI Specs for All Services (doable now)

Theme 1 (Developer Experience) from the old v1.16 plan. Pure code and docs, verifiable in CI, no live endpoint needed. Also gives the already-shipped frontend Swagger UI page real content to render.

- PR 1 (slice 1): OpenAPI 3.0 spec for accounts-service only (`accounts-service/openapi.yaml`, or aide-generated) plus a `docs/API.md` skeleton (getting started, auth model, error envelope, rate limits).
- PR 2 (slice 2): OpenAPI specs for the remaining workspace services following the slice-1 pattern (contacts, activities, automation, integrations, opportunities, reporting, search; include projects, audit, and spend if the pattern holds).
- Done when: every spec passes a validation step in CI (spectral or swagger-cli or equivalent) and `docs/API.md` links all specs. Frontend wiring of the Swagger UI page is tracked in the infraportal roadmap, not here.

### v1.16.2 - Gateway Aggregator and Rate-Limit Guide (doable now)

Completes the API-surface documentation.

- PR 1: `docs/RATE_LIMITING.md` in this repo: header interpretation, Retry-After, backoff strategies, and the per-route tier limits shipped in v1.10 (auth 5 rps, write 30 rps, read 60 rps, X-RateLimit-* response headers).
- PR 2: go-gateway `/api/openapi.json` spec aggregator route (separate repo at `d:/Projects/Portfolio/go-gateway`, not this workspace), code plus unit tests only. The gateway is offline; never verify against a live URL.
- Done when: guide is committed and the gateway unit tests pass in CI.

### v1.16.3 - TypeScript SDK and Postman Collection (doable now, depends on v1.16.1)

Finishes Theme 1: generated artifacts from the OpenAPI specs, build-only verification.

- PR 1: TypeScript SDK generated from the OpenAPI specs into `sdks/typescript-sdk/` with usage examples. BUILD only; publishing to npm is USER-ONLY.
- PR 2: Postman collection generated from the specs, plus `.env.postman_example` documenting required variables.
- Done when: the SDK compiles (tsc) in CI and the Postman collection imports cleanly.

### v1.16.4 - Portal Bulk Ops Frontend (doable now, mocked API; infraportal repo)

Theme 3 (Portal UX) rescoped for the decommission: frontend-only against a clearly marked mocked API layer. The infraportal repo still deploys via GitHub Pages on merge to main, so this work ships for real. Full scope and done-when in `d:/Projects/Portfolio/infraportal/ROADMAP.md`.

### v1.16.5 - Deliverable Templates and Project Cloning Frontend (doable now, mocked API; infraportal repo)

Second Theme 3 slice using the same mocked-API pattern; keeps the one-minor-per-week cadence with small increments. Full scope and done-when in `d:/Projects/Portfolio/infraportal/ROADMAP.md`.

After v1.16.5: the v1.17 theme (Interactive API Playground, [`docs/design/V1_17_THEME.md`](docs/design/V1_17_THEME.md)) was approved with defaults on 2026-07-19 and DELIVERED the same day: v1.17.1 spec rendering + restored nav (infraportal PR #23), v1.17.2 Try it builder with 28 executable operations (PR #24), v1.17.3 snippets + deep links (PR #25), v1.17.4 cross-repo drift detection + patch notes (PR #26). The live playground is at the portfolio site's API Docs page.

---

## Later / candidates (not scheduled)

- Quick win (doable now): find and fix the stale DynamoDB blurb on the pinned portfolio repo README. Locate the repo first; likely backend-service or infraportal (which has `DynamoDbCaseStudyPage.tsx`).
- Docs truth pass (doable now): correct this repo's `README.md` (still claims backend-service uses SQLite, migrated to PostgreSQL in v1.5.0 on 2026-05-08; its service list names only 2 of the 11 workspace service crates, accounts and contacts, alongside 4 non-workspace services) and the `CLAUDE.md` architecture prose (claims live Cloud SQL / Cloud Run; gives the frontend location as `d:/Projects/microservices/frontend-service` when the actual repo is `d:/Projects/Portfolio/infraportal`) to note the 2026-06-04 decommission and offline status.
- Frontend follow-up (doable once v1.16.1/.2 specs exist): restore the API Docs nav link and point the Swagger UI page at the committed specs. Tracked in the infraportal roadmap.

---

## BLOCKED (do not pick up)

- **Cost Intelligence** (old plan v1.16.5 through v1.16.7: GCP Billing dashboards, budget alerts, anomaly detection, Recommender API): BLOCKED, requires live GCP billing data, which no longer exists after the 2026-06-04 decommission.
- **Client email notifications and activity feed backend** (old plan v1.16.10): BLOCKED, requires a live backend and email service.
- **Anything requiring live infrastructure**: Cloud Run or Fly redeploys, live-endpoint verification, canary / SLO / chaos-runbook exercises, Cloud SQL recreation. BLOCKED behind the infra-rebuild decision, which is USER-ONLY and has not been made.

---

## USER-ONLY

- The infra-rebuild decision itself (and any paid GCP / Fly account actions it implies).
- npm publish of the TypeScript SDK (v1.16.3 builds it; the user publishes it).
- GitHub release tags (item 6 of the Release Locations checklist in `CLAUDE.md`); the autonomous workflow updates docs and patch notes but never tags or creates releases.
- Stripe or any paid-account actions.
- Publishing the drafted LinkedIn post.

---

## History and supersession

- The v1.16 plan existed only in private notes and was never committed. Its themes: Theme 1 Developer Experience (OpenAPI specs, gateway aggregator, rate-limit guide, TypeScript SDK, Postman collection), Theme 2 Cost Intelligence, Theme 3 Portal UX. After the 2026-06-04 decommission, Theme 2 is BLOCKED outright, Theme 3 is rescoped to mocked-API frontend work, and Theme 1 is doable as-is.
- Numbering supersession (old private-plan numbers to this roadmap): old v1.16.1-.4 (Theme 1) became v1.16.1 (specs), v1.16.2 (aggregator + rate-limit guide), and v1.16.3 (SDK + Postman); old v1.16.5-.7 (Cost Intelligence) is BLOCKED and unscheduled; old v1.16.8-.9 (portal bulk ops, templates) became v1.16.4 and v1.16.5; old v1.16.10 (client notifications) is BLOCKED. The autodev backlog at `d:/Projects/.claude/skills/autodev/backlogs/portfolio.md` still uses the old numbering and should be synced to this roadmap.
- The frontend shipped a "v1.16.1"-labeled API docs page with Swagger UI on 2026-06-01 under the old numbering, out of order; the service-side specs it was meant to display were never started, so the page currently has nothing real to render, and its nav link was removed during the 2026-06-26 monetization pivot. New v1.16.1/.2 supply the content; the nav restore is a frontend follow-up.
- `V1.9-IMPLEMENTATION.md` is a historical implementation guide; v1.9 shipped 2026-05-07 and the unchecked boxes in that file do not represent open work.
- The infraportal product direction pivoted from CRM demo portal to a consulting monetization funnel between 2026-06-23 and 2026-06-26 with no planning-doc record at the time; the funnel is now a first-class surface in the infraportal roadmap.
