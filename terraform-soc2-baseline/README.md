# terraform-soc2-baseline

A **cloud-agnostic** Terraform module that encodes SOC 2 Type II security controls as reusable infrastructure-as-code.

Extracted from the InfraPortal v0.2 security hardening release. Works on **GCP** and **AWS** via parallel sub-modules with an identical control surface.

---

## Quick Start

### GCP
```hcl
module "soc2" {
  source = "github.com/rodmen07/microservices//terraform-soc2-baseline/modules/gcp"

  project_id      = "your-project-id"
  region          = "us-central1"
  services        = ["accounts", "contacts", "backend"]
  log_bucket_name = "your-org-audit-logs"
}
```

### AWS
```hcl
module "soc2" {
  source = "github.com/rodmen07/microservices//terraform-soc2-baseline/modules/aws"

  region            = "us-east-1"
  environment       = "production"
  services          = ["accounts", "contacts", "backend"]
  trail_bucket_name = "your-org-cloudtrail"
}
```

See [`examples/gcp-complete/`](examples/gcp-complete/) and [`examples/aws-complete/`](examples/aws-complete/) for full working configurations.

---

## What This Module Provides

Each cloud sub-module creates:

| Resource | GCP | AWS |
|----------|-----|-----|
| Per-service identity | Service Account per service | IAM Role per service |
| CI/CD identity | WIF-ready SA (no key files) | OIDC role (no access keys) |
| Secrets store | Secret Manager (auto-replicated) | Secrets Manager + KMS CMK |
| Network | VPC + private subnet + Cloud SQL private path | VPC + private/public subnets + NAT GW |
| Audit trail | Cloud Audit Logs + GCS sink | CloudTrail (multi-region) + S3 |
| Container registry | Artifact Registry | ECR (immutable tags + scan on push) |
| Non-root enforcement | Dockerfile USER policy (documented) | ECS task def `user: "65534"` |

---

## SOC 2 Type II Compliance Mapping

| Control | Description | GCP Implementation | AWS Implementation | Evidence File |
|---------|-------------|-------------------|-------------------|---------------|
| **CC6.1** | Logical and physical access controls | Per-service SA; no `roles/owner` or `roles/editor` granted | Per-service IAM role; resource-scoped ARNs; no wildcard actions | `modules/gcp/iam.tf`, `modules/aws/iam.tf` |
| **CC6.2** | Authentication mechanisms | Workload Identity Federation — no SA key files issued | OIDC role assumption — no long-lived access keys | `modules/gcp/iam.tf`, `modules/aws/iam.tf` |
| **CC6.3** | Privileged access management | `roles/cloudsql.client` + `roles/secretmanager.secretAccessor` only | Inline policies scoped to exact resource ARNs; no `*` actions | `modules/gcp/iam.tf`, `modules/aws/iam.tf` |
| **CC6.7** | Confidential data protection | Secret Manager auto-replication; SA-bound IAM; `prevent_destroy = true` | Secrets Manager + KMS CMK with key rotation; resource policy | `modules/gcp/secrets.tf`, `modules/aws/secrets.tf` |
| **CC6.8** | Non-root container controls | Artifact Registry; Dockerfile USER requirement documented + CI lint check | ECR immutable tags; ECS task `user: "65534"`, `privileged: false`, `drop: ["ALL"]` | `modules/gcp/containers.tf`, `modules/aws/containers.tf` |
| **CC7.2** | System monitoring and logging | Cloud Audit Logs DATA_READ/WRITE for SecretManager, CloudSQL, CloudRun; GCS sink | CloudTrail multi-region, log file validation, S3 versioning + KMS | `modules/gcp/audit.tf`, `modules/aws/audit.tf` |
| **CC7.3** | Incident detection | Cloud Monitoring alert on Secret Manager access spike | CloudWatch alarm on root account usage | `modules/gcp/audit.tf`, `modules/aws/audit.tf` |
| **CC8.1** | Change management | `prevent_destroy` on secrets; state backend note in README | S3 versioning on trail bucket; DynamoDB state lock pattern | `modules/*/secrets.tf`, `modules/*/audit.tf` |
| **A1.2** | Availability commitments | Cloud Run `min_instances`; Cloud SQL backups enabled | Multi-AZ subnets; ECS `desired_count = 2`; `deployment_minimum_healthy_percent = 100` | `modules/gcp/containers.tf`, `modules/aws/containers.tf` |

---

## Prerequisites

### GCP
- Terraform >= 1.5
- `google` provider ~> 5.0
- GCP project with billing enabled
- Caller must have `roles/owner` or `roles/resourcemanager.projectIamAdmin` during initial apply
- After apply: configure Workload Identity Pool and bind to the `cicd-deployer` SA

### AWS
- Terraform >= 1.5
- `aws` provider ~> 5.0
- IAM user/role with `AdministratorAccess` for initial apply (then lock down)
- OIDC provider for `token.actions.githubusercontent.com` must exist in the account
  — update `YOUR_GITHUB_ORG/YOUR_REPO` in `modules/aws/iam.tf` before apply

---

## State Backend Recommendation

**Never store Terraform state locally in production.** Use a remote backend:

```hcl
# GCP
terraform {
  backend "gcs" {
    bucket = "your-org-tf-state"
    prefix = "soc2-baseline"
  }
}

# AWS
terraform {
  backend "s3" {
    bucket         = "your-org-tf-state"
    key            = "soc2-baseline/terraform.tfstate"
    region         = "us-east-1"
    dynamodb_table = "terraform-state-lock"
    encrypt        = true
  }
}
```

---

## Setting Secret Values

This module creates secret resources but does NOT store secret values in state.
Set values out-of-band after `terraform apply`:

```bash
# GCP
echo -n "your-jwt-secret" | gcloud secrets versions add AUTH_JWT_SECRET --data-file=-

# AWS
aws secretsmanager put-secret-value \
  --secret-id "production/auth/jwt-secret" \
  --secret-string "your-jwt-secret"
```

---

## Related Projects

- [InfraPortal microservices platform](../README.md) — the system this module was extracted from
- [CI/CD Pipeline Template](docs/cicd-template/README.md) — multi-environment promotion with automated rollback
