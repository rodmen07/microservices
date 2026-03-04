# automation-service

Workflow automation domain microservice for CRM platform development.

## Purpose

This service owns workflow definitions that react to events and trigger actions.

## Endpoints

- `GET /health`
- `GET /ready`
- `GET /api/v1/workflows`
- `POST /api/v1/workflows`
- `GET /api/v1/workflows/{id}`
- `PATCH /api/v1/workflows/{id}`
- `DELETE /api/v1/workflows/{id}`

## Run locally

```bash
cargo run
```

Defaults:

- `HOST=0.0.0.0`
- `PORT=3014`

## Example payloads

Create workflow:

```json
{
  "name": "New lead follow-up",
  "trigger_event": "contact.created",
  "action_type": "create_task"
}
```

## Notes

- Current persistence is in-memory for rapid prototyping.
- Next step: add execution engine + durable job queue.
