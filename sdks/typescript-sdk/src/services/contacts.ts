/**
 * Typed contacts-service module: list/get/create/update/delete over
 * /api/v1/contacts, using the types generated from contacts-service/openapi.yaml.
 *
 * Routes and response types are pinned to the generated spec types by the
 * helpers in ../core/routes.js.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import { expandPath } from "../core/routes.js";
import type {
  AssertNoUncoveredRoutes,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type { components, operations, paths } from "../generated/contacts.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/contacts",
  "/api/v1/contacts/{id}",
] as const satisfies readonly (keyof paths)[];

const [CONTACTS, CONTACT] = ROUTES;

/** Fails the build if contacts-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

export type Contact = components["schemas"]["Contact"];
export type ContactLifecycleStage = Contact["lifecycle_stage"];
export type CreateContactRequest = components["schemas"]["CreateContactRequest"];
export type UpdateContactRequest = components["schemas"]["UpdateContactRequest"];
export type ListContactsResponse = JsonBody<operations["listContacts"], 200>;
export type ListContactsQuery = NonNullable<
  operations["listContacts"]["parameters"]["query"]
>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class ContactsApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/contacts. Paginated, and filterable by account, lifecycle
   * stage, owner, or a free-text `q`. */
  list(
    query?: ListContactsQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<ListContactsResponse>> {
    return this.client.request<ListContactsResponse>("GET", CONTACTS, {
      ...options,
      query,
    });
  }

  /** GET /api/v1/contacts/{id}. Throws ApiError NOT_FOUND (404) when absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<Contact>> {
    return this.client.request<Contact>("GET", expandPath(CONTACT, { id }), {
      ...options,
    });
  }

  /** POST /api/v1/contacts. Returns 201 with the created contact. Retried
   * automatically only on 429; pass `idempotent: true` to also retry network
   * errors if your create flow tolerates duplicates. */
  create(
    body: CreateContactRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Contact>> {
    return this.client.request<Contact>("POST", CONTACTS, {
      ...options,
      body,
    });
  }

  /** PATCH /api/v1/contacts/{id}. Partial update; omitted fields keep their
   * stored values. Retried automatically only on 429. */
  update(
    id: string,
    body: UpdateContactRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<Contact>> {
    return this.client.request<Contact>("PATCH", expandPath(CONTACT, { id }), {
      ...options,
      body,
    });
  }

  /** DELETE /api/v1/contacts/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(CONTACT, { id }), {
      ...options,
    });
  }
}
