/**
 * @rodmen07/infraportal-sdk
 *
 * TypeScript SDK for the InfraPortal CRM platform API. See README.md for
 * usage, the retry contract, and the current runtime status of the platform.
 */
export {
  ApiError,
  DEFAULT_RETRY_OPTIONS,
  InfraPortalClient,
  UNKNOWN_ERROR_CODE,
  parseErrorEnvelope,
  parseRateLimit,
  parseRetryAfterSeconds,
} from "./core/client.js";
export type {
  ApiErrorEnvelope,
  ApiResponse,
  ClientOptions,
  QueryValue,
  RateLimitInfo,
  RequestOptions,
  RetryOptions,
} from "./core/client.js";

export { expandPath } from "./core/routes.js";
export type {
  Assert,
  AssertNoUncoveredRoutes,
  DeclaresNoParameters,
  JsonBody,
  PathParamNames,
  PathParams,
  ProbePath,
} from "./core/routes.js";

// Typed service modules. `CallOptions` is identical in every one of them, so it
// is re-exported once here rather than seven times under colliding names.
export type { CallOptions } from "./services/accounts.js";

export { AccountsApi } from "./services/accounts.js";
export type {
  Account,
  AccountStatus,
  CreateAccountRequest,
  ListAccountsQuery,
  ListAccountsResponse,
  UpdateAccountRequest,
} from "./services/accounts.js";

export { ActivitiesApi } from "./services/activities.js";
export type {
  Activity,
  CreateActivityRequest,
  ListActivitiesResponse,
  UpdateActivityRequest,
} from "./services/activities.js";

export { AuditApi } from "./services/audit.js";
export type {
  AuditEvent,
  CreateAuditEventRequest,
  ListAuditEventsQuery,
  ListAuditEventsResponse,
} from "./services/audit.js";

export { AutomationApi } from "./services/automation.js";
export type {
  CreateWorkflowRequest,
  ListWorkflowsResponse,
  UpdateWorkflowRequest,
  Workflow,
} from "./services/automation.js";

export { ContactsApi } from "./services/contacts.js";
export type {
  Contact,
  ContactLifecycleStage,
  CreateContactRequest,
  ListContactsQuery,
  ListContactsResponse,
  UpdateContactRequest,
} from "./services/contacts.js";

export { IntegrationsApi } from "./services/integrations.js";
export type {
  CreateConnectionRequest,
  IntegrationConnection,
  ListConnectionsResponse,
  UpdateConnectionRequest,
} from "./services/integrations.js";

export { OpportunitiesApi } from "./services/opportunities.js";
export type {
  CreateOpportunityRequest,
  ListOpportunitiesQuery,
  ListOpportunitiesResponse,
  Opportunity,
  UpdateOpportunityRequest,
} from "./services/opportunities.js";
