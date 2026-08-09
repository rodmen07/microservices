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
  AnyBody,
  Assert,
  AssertNoUncoveredRoutes,
  DeclaresNoParameters,
  DeclaresRequiredQuery,
  JsonBody,
  PathParamNames,
  PathParams,
  ProbePath,
} from "./core/routes.js";

// Typed service modules, one per workspace service — all eleven are covered.
// `CallOptions` is identical in every one of them, so it is re-exported once
// here rather than eleven times under colliding names.
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

export { ProjectsApi } from "./services/projects.js";
export type {
  CreateDeliverableRequest,
  CreateMessageRequest,
  CreateMilestoneRequest,
  CreateProjectLinkRequest,
  CreateProjectRequest,
  Deliverable,
  ListDeliverablesResponse,
  ListEmailsResponse,
  ListLinksResponse,
  ListMessagesResponse,
  ListMilestonesResponse,
  ListProjectsResponse,
  Message,
  Milestone,
  Project,
  ProjectEmail,
  ProjectLink,
  SyncEmailsRequest,
  SyncEmailsResponse,
  UpdateDeliverableRequest,
  UpdateMilestoneRequest,
  UpdateProjectRequest,
} from "./services/projects.js";

export { ReportingApi } from "./services/reporting.js";
export type {
  CreateReportRequest,
  DashboardSummary,
  DashboardView,
  ExportReportsQuery,
  ExportReportsResponse,
  GetDashboardQuery,
  ListReportsResponse,
  SavedReport,
  UpdateReportRequest,
} from "./services/reporting.js";

export { SearchApi } from "./services/search.js";
export type {
  IndexDocumentRequest,
  ListDocumentsResponse,
  SearchDocument,
  SearchQuery,
  SearchResponse,
  SearchResult,
} from "./services/search.js";

export { SpendApi } from "./services/spend.js";
export type {
  CreateSpendRequest,
  ListSpendQuery,
  ListSpendResponse,
  SpendRecord,
  SpendSummary,
  SpendSummaryQuery,
  SyncResult,
  UpdateSpendRequest,
} from "./services/spend.js";
