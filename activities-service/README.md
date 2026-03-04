# activities-service

Activity timeline domain microservice for CRM platform development.

## Purpose

This service owns activity records (calls, emails, meetings, tasks) tied to accounts/contacts.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/activities`
- `POST /api/v1/activities`
- `GET /api/v1/activities/{id}`
- `PATCH /api/v1/activities/{id}`
- `DELETE /api/v1/activities/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3013`

## Example payloads

Create activity:

```json
{
  "account_id": null,
  "contact_id": null,
  "activity_type": "call",
  "subject": "Discovery call",
  "notes": "Discussed onboarding timeline",
  "due_at": "2026-03-05T17:00:00Z"
}
```

## Notes

- Current persistence is in-memory for rapid prototyping.
- Next step: add reminder engine and activity feed ordering/indexing.
