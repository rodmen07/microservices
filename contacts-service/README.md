# contacts-service

Contacts/leads domain microservice for CRM platform development.

## Purpose

This service owns person-level records linked to accounts and tracks lifecycle stage.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/contacts`
- `POST /api/v1/contacts`
- `GET /api/v1/contacts/{id}`
- `PATCH /api/v1/contacts/{id}`
- `DELETE /api/v1/contacts/{id}`

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

- Current persistence is in-memory for rapid prototyping.
- Next step: add deduplication rules and account ownership constraints.
