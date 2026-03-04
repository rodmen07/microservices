# opportunities-service

Opportunities pipeline domain microservice for CRM platform development.

## Purpose

This service owns deal/opportunity lifecycle data: stage tracking, value, and expected close timing.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/opportunities`
- `POST /api/v1/opportunities`
- `GET /api/v1/opportunities/{id}`
- `PATCH /api/v1/opportunities/{id}`
- `DELETE /api/v1/opportunities/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3012`

## Example payloads

Create opportunity:

```json
{
  "account_id": "11111111-1111-1111-1111-111111111111",
  "name": "Q4 Platform Expansion",
  "stage": "qualification",
  "amount": 125000,
  "close_date": "2026-12-20"
}
```

## Notes

- Current persistence is in-memory for rapid prototyping.
- Next step: enforce stage transitions + weighted forecast views.
