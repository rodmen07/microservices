# contacts-service

Contacts/leads domain microservice for CRM platform development.

## Purpose

This service owns person-level records linked to accounts and tracks lifecycle stage.

## Endpoints

- `GET /health` (open)
- `GET /ready` (open)
- `GET /api/v1/contacts` (admin only)
- `POST /api/v1/contacts` (admin only)
- `GET /api/v1/contacts/{id}` (admin only)
- `PATCH /api/v1/contacts/{id}` (admin only)
- `DELETE /api/v1/contacts/{id}` (admin only)

## Auth

All `/api/v1/contacts` routes require a Bearer JWT (validated with `AUTH_JWT_SECRET`,
issuer `AUTH_ISSUER`, default `auth-service`) whose `roles` claim includes `admin`
(case-insensitive). Requests without a valid token get `401`; valid tokens without
the admin role get `403 FORBIDDEN`. `/health` and `/ready` are unauthenticated for
Cloud Run probes. Owner scoping on reads/updates/deletes is kept as defense in depth.

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3011`

## Example payloads

Create contact:

```json
{
  "account_id": null,
  "first_name": "Taylor",
  "last_name": "Reese",
  "email": "taylor@acme.com",
  "phone": "+1-555-0101"
}
```

Update contact:

```json
{
  "lifecycle_stage": "qualified"
}
```

## Notes

- Persistence is PostgreSQL (sqlx, migrations run at startup via `DATABASE_URL`).
- `account_id` on create/update is validated against accounts-service when
  `ACCOUNTS_SERVICE_URL` is set (fail-open when unset), forwarding the caller's token.
- Creates/updates/deletes write through to search-service and emit audit events
  when the corresponding env vars are set.
