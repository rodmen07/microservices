/**
 * Typed search-service module: the query endpoint plus the document index it
 * reads from, using the types generated from search-service/openapi.yaml.
 *
 * Two shapes here are unlike the CRM services:
 *
 * - `/api/v1/search` is the only platform operation with a REQUIRED query
 *   parameter. A request without `q` is answered 400, not with an empty result,
 *   so `search()` takes its query as a required argument and
 *   `_SearchQueryIsRequired` pins the spec side of that decision.
 * - Documents are addressable two ways: by their own id and by the entity they
 *   describe. `deleteDocumentsByEntity` targets the second route, which is a
 *   bulk delete keyed on `entity_id` and returns 204 whether or not anything
 *   matched — it is not a 404-on-missing sibling of `deleteDocument`.
 */
import type {
  ApiResponse,
  InfraPortalClient,
  RequestOptions,
} from "../core/client.js";
import { expandPath } from "../core/routes.js";
import type {
  Assert,
  AssertNoUncoveredRoutes,
  DeclaresNoParameters,
  DeclaresRequiredQuery,
  JsonBody,
  ProbePath,
} from "../core/routes.js";
import type { components, operations, paths } from "../generated/search.js";

/** Every route this module calls, pinned to the generated spec paths. */
const ROUTES = [
  "/api/v1/search",
  "/api/v1/search/documents",
  "/api/v1/search/documents/{id}",
  "/api/v1/search/documents/by-entity/{entity_id}",
] as const satisfies readonly (keyof paths)[];

const [SEARCH, DOCUMENTS, DOCUMENT, DOCUMENTS_BY_ENTITY] = ROUTES;

/** Fails the build if search-service/openapi.yaml gains a route this module
 * does not call. The error names the uncovered path. */
type _AllRoutesCovered = AssertNoUncoveredRoutes<
  Exclude<keyof paths, (typeof ROUTES)[number] | ProbePath>
>;

/** `q` is required by the spec, so `search()` demands it. If the spec ever made
 * the query optional this flips to false and fails the build, rather than
 * leaving a needlessly strict signature nobody re-checked. */
type _SearchQueryIsRequired = Assert<
  DeclaresRequiredQuery<operations["searchDocuments"]>
>;

/** The index listing takes no filter today; a spec that grows one fails here
 * rather than leaving the SDK unable to send it. */
type _ListDeclaresNoParameters = Assert<
  DeclaresNoParameters<operations["listDocuments"]>
>;

export type SearchResult = components["schemas"]["SearchResult"];
export type SearchDocument = components["schemas"]["SearchDocument"];
export type IndexDocumentRequest =
  components["schemas"]["IndexDocumentRequest"];
export type SearchQuery = operations["searchDocuments"]["parameters"]["query"];
export type SearchResponse = JsonBody<operations["searchDocuments"], 200>;
export type ListDocumentsResponse = JsonBody<operations["listDocuments"], 200>;

/** Per-call options: everything except query/body, which the methods own. */
export type CallOptions = Omit<RequestOptions, "query" | "body">;

export class SearchApi {
  constructor(private readonly client: InfraPortalClient) {}

  /** GET /api/v1/search. `q` is required: the service answers 400 when it is
   * missing rather than returning every document. */
  search(
    query: SearchQuery,
    options?: CallOptions,
  ): Promise<ApiResponse<SearchResponse>> {
    return this.client.request<SearchResponse>("GET", SEARCH, {
      ...options,
      query,
    });
  }

  /** GET /api/v1/search/documents. The indexed corpus itself, as a bare array. */
  listDocuments(
    options?: CallOptions,
  ): Promise<ApiResponse<ListDocumentsResponse>> {
    return this.client.request<ListDocumentsResponse>("GET", DOCUMENTS, {
      ...options,
    });
  }

  /** POST /api/v1/search/documents. Returns 201. Retried automatically only
   * on 429. */
  indexDocument(
    body: IndexDocumentRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SearchDocument>> {
    return this.client.request<SearchDocument>("POST", DOCUMENTS, {
      ...options,
      body,
    });
  }

  /** GET /api/v1/search/documents/{id}. Throws ApiError NOT_FOUND (404) when
   * absent. */
  get(id: string, options?: CallOptions): Promise<ApiResponse<SearchDocument>> {
    return this.client.request<SearchDocument>(
      "GET",
      expandPath(DOCUMENT, { id }),
      { ...options },
    );
  }

  /** PATCH /api/v1/search/documents/{id}. The body is an
   * `IndexDocumentRequest`, the same shape indexing takes. Retried
   * automatically only on 429. */
  update(
    id: string,
    body: IndexDocumentRequest,
    options?: CallOptions,
  ): Promise<ApiResponse<SearchDocument>> {
    return this.client.request<SearchDocument>(
      "PATCH",
      expandPath(DOCUMENT, { id }),
      { ...options, body },
    );
  }

  /** DELETE /api/v1/search/documents/{id}. Resolves with no data on 204. */
  delete(id: string, options?: CallOptions): Promise<ApiResponse<void>> {
    return this.client.request<void>("DELETE", expandPath(DOCUMENT, { id }), {
      ...options,
    });
  }

  /** DELETE /api/v1/search/documents/by-entity/{entity_id}. Removes every
   * document indexed for one source entity; 204 whether or not any matched. */
  deleteByEntity(
    entityId: string,
    options?: CallOptions,
  ): Promise<ApiResponse<void>> {
    return this.client.request<void>(
      "DELETE",
      expandPath(DOCUMENTS_BY_ENTITY, { entity_id: entityId }),
      { ...options },
    );
  }
}
