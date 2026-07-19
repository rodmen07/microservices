/**
 * Typed accounts-service module: list/get/create/update/delete over
 * /api/v1/accounts, using the types generated from
 * accounts-service/openapi.yaml.
 *
 * All routes require a bearer JWT with the admin role (docs/API.md); a valid
 * token without it produces ApiError { status: 403, code: "FORBIDDEN" }.
 *
 * This is the first typed service module. The remaining services (activities,
 * audit, automation, contacts, integrations, opportunities, projects,
 * reporting, search, spend) land in follow-up PRs; their generated types are
 * already available under src/generated/.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import type { components, operations } from "../generated/accounts.js";

export type Account = components["schemas"]["Account"];
export type AccountStatus = Account["status"];
export type CreateAccountRequest = components["schemas"]["CreateAccountRequest"];
export type UpdateAccountRequest = components["schemas"]["UpdateAccountRequest"];
export type ListAccountsResponse = components["schemas"]["ListAccountsResponse"];
export type ListAccountsQuery = NonNullable<
  operations["listAccounts"]["parameters"]["query"]
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class AccountsApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/accounts. Paginated; limit is clamped to 1..100 server-side. */
  list(
    query?: ListAccountsQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ListAccountsResponse>> {
    return this.client.request<ListAccountsResponse>("GET", "/api/v1/accounts", {
      ...options,
      query,
    });
  }

  /** GET /api/v1/accounts/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Account>> {
    return this.client.request<Account>(
      "GET",
      `/api/v1/accounts/${encodeURIComponent(id)}`,
      { ...options },
    );
  }

  /**
   * POST /api/v1/accounts. Returns 201 with the created account.
   * As a POST it is retried automatically only on 429 (safe: the gateway
   * rejects before proxying); pass `idempotent: true` to also retry network
   * errors if your create flow tolerates duplicates.
   */
  create(
    body: CreateAccountRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Account>> {
    return this.client.request<Account>("POST", "/api/v1/accounts", {
      ...options,
      body,
    });
  }

  /**
   * PATCH /api/v1/accounts/{id}. Partial update; omitted fields keep their
   * stored values. Retried automatically only on 429 (PATCH is treated as
   * non-idempotent); pass `idempotent: true` to opt in to network-error
   * retries.
   */
  update(
    id: string,
    body: UpdateAccountRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Account>> {
    return this.client.request<Account>(
      "PATCH",
      `/api/v1/accounts/${encodeURIComponent(id)}`,
      { ...options, body },
    );
  }

  /** DELETE /api/v1/accounts/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>(
      "DELETE",
      `/api/v1/accounts/${encodeURIComponent(id)}`,
      { ...options },
    );
  }
}
