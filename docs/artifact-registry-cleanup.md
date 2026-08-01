# Artifact Registry cleanup policy (proposal, measured 2026-08-01)

Every merge to `main` builds and pushes one container image per service to
Artifact Registry, and nothing ever deletes them. This document records the
measured state of that growth and proposes a bounded retention policy. The
policy file lives beside this doc at
[`artifact-registry-cleanup-policy.json`](artifact-registry-cleanup-policy.json).

**Applying the policy mutates cloud infrastructure and is a USER-ONLY action.**
Nothing in CI or the agent workflow applies it. The apply steps below are for
the repository owner.

## Measured state (2026-08-01, read-only `gcloud` commands)

Project `microservices-489413`, all locations:

| Fact | Value | Command |
|---|---|---|
| Repositories | 1 (`us-central1/microservices`, DOCKER) | `gcloud artifacts repositories list` |
| Repository size | 1950.591 MB (list view lagged at 1860.229 MB) | `gcloud artifacts repositories describe microservices --location=us-central1` |
| Created | 2026-06-23 | same `list` call |
| Image versions | **259** across 13 packages | `gcloud artifacts docker images list us-central1-docker.pkg.dev/microservices-489413/microservices` |
| Cleanup policies | **none defined** | `describe --format="yaml(cleanupPolicies,cleanupPolicyDryRun)"` returns no `cleanupPolicies` key |
| Dry-run flag | `cleanupPolicyDryRun: true` already set | same command |

Versions per package: projects-service 23; accounts, activities, audit,
automation, integrations, opportunities, reporting, search, spend 22 each;
contacts-service 21; go-gateway 15; auth-service 2.

## Cost math (honest version)

Artifact Registry storage is $0.10/GB/month after a 0.5 GB free tier.

- Today: 1.95 GB, so roughly **$0.15/month**. Trivial in dollars.
- Growth: 1.95 GB accrued in 39 days (repo created 2026-06-23), roughly
  **1.5 GB/month** at the July merge rate. Left alone that is ~19 GB and
  ~$1.90/month within a year, growing linearly and forever.

The conclusion the backlog item asked for: current cost is NOT significant.
The value of a policy is bounding unbounded growth cheaply, not recovering
current spend, and the cheap moment to bound it is before it compounds.

## Proposed policy

Two rules, standard Artifact Registry semantics (Keep overrides Delete):

1. **Keep** the 10 most recent versions of every package.
2. **Delete** any version older than 30 days (2592000s).

Net effect per service: everything newer than 30 days survives, plus always
the 10 newest versions regardless of age. Steady state is ~10 versions per
service plus the recent-merge tail, roughly half of today's 259 versions,
and growth stops compounding.

Rollback depth: Cloud Run revisions pin image digests, so deleting a version
only breaks re-deploying revisions older than the retained window. Ten
versions is ten merges of rollback depth per service, far deeper than any
rollback this repo has ever needed.

## Apply steps (USER-ONLY, owner runs these)

The repo-level dry-run flag is already `true`, so step 1 cannot delete
anything by itself.

```sh
# 1. Set the policies with dry-run kept on. Nothing is deleted.
gcloud artifacts repositories set-cleanup-policies microservices \
  --location=us-central1 --project=microservices-489413 \
  --policy=docs/artifact-registry-cleanup-policy.json \
  --dry-run

# 2. After a day, inspect what WOULD have been deleted (cleanup runs are
#    asynchronous; dry-run candidates appear in the audit log):
gcloud logging read \
  'protoPayload.methodName:"ArtifactRegistry" protoPayload.methodName:"Delete"' \
  --project=microservices-489413 --limit=50

# 3. If the candidate list looks right, activate:
gcloud artifacts repositories set-cleanup-policies microservices \
  --location=us-central1 --project=microservices-489413 \
  --policy=docs/artifact-registry-cleanup-policy.json \
  --no-dry-run

# 4. Verify:
gcloud artifacts repositories describe microservices --location=us-central1 \
  --format="yaml(cleanupPolicies,cleanupPolicyDryRun)"
```

After activation, re-measure size and version count (the two commands in the
table above) and record the after numbers in the autodev backlog COST item,
which carries this proposal's close condition.
