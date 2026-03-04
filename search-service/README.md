# search-service

Cross-entity search domain microservice for CRM platform development.

## Purpose

This service owns indexing/search for CRM entities to support global query experiences.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/search?q=...`
- `GET /api/v1/search/documents`
- `POST /api/v1/search/documents`
- `GET /api/v1/search/documents/{id}`
- `PATCH /api/v1/search/documents/{id}`
- `DELETE /api/v1/search/documents/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3016`

## Notes

- Current indexing is in-memory for rapid prototyping.
- Next step: move to dedicated search backend and incremental indexing workers.
