# accounts-service

Accounts/tenant domain microservice for CRM platform development.

## Purpose

This service owns account-level entities (organizations/customers) and account lifecycle status.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/accounts`
- `POST /api/v1/accounts`
- `GET /api/v1/accounts/{id}`
- `PATCH /api/v1/accounts/{id}`
- `DELETE /api/v1/accounts/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3010`

## Example payloads

Create account:

```json
{
  "name": "Acme Corp",
  "domain": "acme.com"
}
```

Update account:

```json
{
  "status": "customer"
}
```

## Notes

- Current persistence is in-memory for rapid prototyping.
- Next step: migrate to SQLite/Postgres with tenant-safe indexing.
