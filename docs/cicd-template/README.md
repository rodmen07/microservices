# CI/CD Pipeline Template

A **cloud-agnostic** GitHub Actions reference architecture for multi-environment deployments.
Supports **GCP Cloud Run** and **AWS ECS/Fargate** — configure one or both in the same workflow.

---

## Architecture

```
  PR / push to main
        │
        ▼
   ┌─────────┐
   │  test   │  Rust clippy + tests + cargo-audit + Dockerfile USER lint
   └────┬────┘
        │ (push to main only)
        ▼
   ┌────────────────┐
   │ deploy-staging │  OIDC auth → build → push → deploy → health-check
   │  (automatic)   │                                           │
   └────────────────┘                               ┌──────────┴──────────┐
        │ success                                   │  rollback on fail   │
        ▼                                           │  GCP: traffic shift │
   ┌─────────────────────────────┐                  │  AWS: prev task def │
   │  ⏸ Awaiting approval       │                  └─────────────────────┘
   │  (GitHub Environment gate) │
   └────────────────┬────────────┘
        │ approved
        ▼
   ┌────────────────┐
   │  deploy-prod   │  OIDC auth → promote image → deploy → health-check
   │  (manual gate) │                                           │
   └────────────────┘                               ┌──────────┴──────────┐
                                                    │  rollback on fail   │
                                                    └─────────────────────┘
```

---

## Setup Checklist

### 1. Create GitHub Environments

In your repo: **Settings → Environments → New environment**

| Environment | Protection rules |
|-------------|-----------------|
| `staging` | None (auto-deploy) |
| `production` | Required reviewers: add yourself or your team |

### 2. Add secrets to each environment

Go to each environment's settings and add:

| Secret | Description |
|--------|-------------|
| `GCP_WORKLOAD_IDENTITY_PROVIDER` | WIF provider: `projects/PROJECT_NUM/locations/global/workloadIdentityPools/POOL/providers/PROVIDER` |
| `GCP_SERVICE_ACCOUNT` | Deployer SA email from `terraform-soc2-baseline` output `cicd_service_account_email` |
| `GCP_SERVICE_URL` | Your Cloud Run service URL (for health check) |
| `AWS_ROLE_TO_ASSUME` | IAM role ARN from `terraform-soc2-baseline` output `cicd_role_arn` |
| `AWS_ECS_CLUSTER` | ECS cluster name from `terraform-soc2-baseline` output `ecs_cluster_name` |
| `AWS_ECS_SERVICE` | Your ECS service name |

> Staging and production use **isolated credentials** — compromising staging secrets does not affect production.

### 3. Add repository variables

**Settings → Variables → Actions → New repository variable**

| Variable | Example |
|----------|---------|
| `GCP_REGION` | `us-central1` |
| `GCP_PROJECT_ID` | `my-project-123` |
| `AWS_REGION` | `us-east-1` |
| `ECR_REGISTRY` | `123456789012.dkr.ecr.us-east-1.amazonaws.com` |
| `ECR_REPOSITORY` | `infraportal` |

### 4. Set up OIDC (one-time)

**GCP:** Create a Workload Identity Pool in your GCP project and bind it to the `cicd-deployer` service account created by `terraform-soc2-baseline`. See [Google's WIF guide](https://cloud.google.com/iam/docs/workload-identity-federation-with-deployment-pipelines).

**AWS:** Ensure the OIDC provider `token.actions.githubusercontent.com` exists in your AWS account. Update `YOUR_GITHUB_ORG/YOUR_REPO` in `modules/aws/iam.tf` before applying `terraform-soc2-baseline`.

---

## Rollback Behavior

Rollback triggers automatically if the health check step fails after deployment.

| Cloud | Mechanism | Effect |
|-------|-----------|--------|
| GCP Cloud Run | `gcloud run services update-traffic --to-revisions PREVIOUS=100` | Instantly shifts 100% traffic back to the previous revision |
| AWS ECS | `aws ecs update-service --task-definition <previous ARN>` | Replaces the service with the last known-good task definition |

The rollback scripts (`scripts/rollback-gcp.sh`, `scripts/rollback-aws.sh`) share the same interface — both accept `SERVICE`, `REGION`, and optionally `CLUSTER` — making them easy to call from any CI/CD system.

---

## Health Check

`scripts/health-check.sh` polls `SERVICE_URL/health` every 10 seconds.

| Environment | Timeout |
|-------------|---------|
| staging | 90s |
| production | 120s |

The script exits 0 on the first HTTP 200 response, or exits 1 (triggering rollback) after the timeout.

---

## Dockerfile Non-Root Enforcement

The `test` job includes a step that scans all Dockerfiles for a `USER` directive in the final stage. Any service missing `USER` fails the build before any deployment occurs. This enforces CC6.8 from the SOC 2 baseline module.

---

## Related Projects

- [terraform-soc2-baseline](../../terraform-soc2-baseline/README.md) — provisions the IAM roles and OIDC identities used by this pipeline
