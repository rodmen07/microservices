# reporting-service

Analytics/reporting domain microservice for CRM platform development.

## Purpose

This service owns saved reports, dashboard summary projections, and reporting metadata.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/reports/dashboard`
- `GET /api/v1/reports`
- `POST /api/v1/reports`
- `GET /api/v1/reports/{id}`
- `PATCH /api/v1/reports/{id}`
- `DELETE /api/v1/reports/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3017`

## Notes

- Current report state is in-memory for rapid prototyping.
- Next step: materialized metric tables + export pipeline.
