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

export { AccountsApi } from "./services/accounts.js";
export type {
  Account,
  AccountStatus,
  CallOptions,
  CreateAccountRequest,
  ListAccountsQuery,
  ListAccountsResponse,
  UpdateAccountRequest,
} from "./services/accounts.js";
