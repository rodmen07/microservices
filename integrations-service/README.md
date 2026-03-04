# integrations-service

External integration connectors microservice for CRM platform development.

## Purpose

This service owns integration connection state (provider/account binding + sync status).

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/integrations/connections`
- `POST /api/v1/integrations/connections`
- `GET /api/v1/integrations/connections/{id}`
- `PATCH /api/v1/integrations/connections/{id}`
- `DELETE /api/v1/integrations/connections/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3015`

## Example payloads

Create connection:

```json
{
  "provider": "gmail",
  "account_ref": "user@example.com"
}
```

## Notes

- Current persistence is in-memory for rapid prototyping.
- Next step: OAuth token vaulting + incremental sync pipelines.
