/**
 * Hand-written fetch client for the InfraPortal platform API.
 *
 * Implements the platform contracts documented in the microservices repo:
 * - docs/API.md: bearer JWT auth, the ApiError envelope, 401/403 semantics.
 * - docs/RATE_LIMITING.md: X-RateLimit-* headers, the 429 response, and the
 *   retry rules (Retry-After first, else capped full-jitter exponential
 *   backoff, bounded retry budget, retry only 429 and network errors).
 */

/** Parsed X-RateLimit-* headers. All fields are null when the header is
 * absent: requests that bypass the gateway carry no rate-limit headers, and
 * the Redis limiter fails open without them, so clients must tolerate
 * their absence. */
export interface RateLimitInfo {
  /** X-RateLimit-Limit: requests per second allowed for the matched tier. */
  limit: number | null;
  /** X-RateLimit-Remaining: whole tokens left in the bucket (advisory). */
  remaining: number | null;
  /** X-RateLimit-Reset: Unix epoch second at which capacity is next available. */
  reset: number | null;
}

/** Extracts the X-RateLimit-* headers from a response. Missing or
 * non-numeric values parse to null. */
export function parseRateLimit(headers: Headers): RateLimitInfo {
  const read = (name: string): number | null => {
    const raw = headers.get(name);
    if (raw === null || raw.trim() === "") return null;
    const value = Number(raw);
    return Number.isFinite(value) ? value : null;
  };
  return {
    limit: read("X-RateLimit-Limit"),
    remaining: read("X-RateLimit-Remaining"),
    reset: read("X-RateLimit-Reset"),
  };
}

/** Parses Retry-After as delay-seconds. The gateway always sends an integer
 * number of seconds, never an HTTP-date (docs/RATE_LIMITING.md), so anything
 * that does not parse to a positive finite number returns null and the
 * caller falls back to jittered backoff. */
export function parseRetryAfterSeconds(headers: Headers): number | null {
  const raw = headers.get("Retry-After");
  if (raw === null) return null;
  const value = Number(raw);
  return Number.isFinite(value) && value > 0 ? value : null;
}

/** The platform error envelope: { code, message, details? }. `details` is
 * omitted when absent, never null. */
export interface ApiErrorEnvelope {
  code: string;
  message: string;
  details?: unknown;
}

/**
 * Parses an already-JSON-parsed body into the platform error envelope.
 * Returns null when the body is not envelope-shaped (for example the
 * text/plain axum rejection bodies documented per operation in the specs).
 *
 * Only `code` is load-bearing: per docs/RATE_LIMITING.md the `message` text
 * is for humans and its exact wording differs between implementations, so
 * SDK logic must never branch on it.
 */
export function parseErrorEnvelope(body: unknown): ApiErrorEnvelope | null {
  if (typeof body !== "object" || body === null || Array.isArray(body)) return null;
  const record = body as Record<string, unknown>;
  if (typeof record["code"] !== "string") return null;
  const envelope: ApiErrorEnvelope = {
    code: record["code"],
    message: typeof record["message"] === "string" ? record["message"] : "",
  };
  if ("details" in record) envelope.details = record["details"];
  return envelope;
}

/** Error code used when a non-2xx response body is not the ApiError
 * envelope (for example axum's text/plain 400/422 rejections). */
export const UNKNOWN_ERROR_CODE = "UNKNOWN";

/** A non-2xx response, carrying the platform ApiError envelope fields plus
 * the HTTP status, rate-limit headers, and Retry-After when present. */
export class ApiError extends Error {
  override readonly name = "ApiError";
  /** HTTP status code of the response. */
  readonly status: number;
  /** Machine-readable envelope code (AUTH_REQUIRED, FORBIDDEN, NOT_FOUND,
   * VALIDATION_ERROR, RATE_LIMITED, DB_ERROR, ...). UNKNOWN when the body
   * was not the envelope. This is the only field to branch on. */
  readonly code: string;
  /** Optional structured context from the envelope. */
  readonly details?: unknown;
  /** Parsed X-RateLimit-* headers from the error response. */
  readonly rateLimit: RateLimitInfo;
  /** Parsed Retry-After header (seconds), null when absent. */
  readonly retryAfterSeconds: number | null;

  constructor(args: {
    status: number;
    code: string;
    message: string;
    details?: unknown;
    rateLimit: RateLimitInfo;
    retryAfterSeconds: number | null;
  }) {
    super(args.message);
    this.status = args.status;
    this.code = args.code;
    if ("details" in args) this.details = args.details;
    this.rateLimit = args.rateLimit;
    this.retryAfterSeconds = args.retryAfterSeconds;
  }
}

/** Retry tuning. Defaults follow docs/RATE_LIMITING.md. */
export interface RetryOptions {
  /** Retries after the initial attempt (so maxRetries 5 means at most 6
   * requests total). Default 5. */
  maxRetries: number;
  /** Base backoff delay in milliseconds. Default 500. */
  baseDelayMs: number;
  /** Backoff cap in milliseconds. Default 8000. */
  maxDelayMs: number;
}

export const DEFAULT_RETRY_OPTIONS: RetryOptions = {
  maxRetries: 5,
  baseDelayMs: 500,
  maxDelayMs: 8000,
};

export interface ClientOptions {
  /** Base URL of the gateway (or a service directly, for local dev). */
  baseUrl: string;
  /** Bearer JWT. Sent as `Authorization: Bearer <token>` on every request.
   * Note the CRM services also require the `admin` role in the token's
   * roles claim; a valid token without it receives 403 FORBIDDEN. */
  token?: string;
  /** Custom fetch implementation. Defaults to globalThis.fetch. */
  fetch?: typeof globalThis.fetch;
  /** Retry tuning overrides. */
  retry?: Partial<RetryOptions>;
  /** Injectable sleep, for tests. Defaults to setTimeout. */
  sleep?: (ms: number) => Promise<void>;
  /** Injectable RNG in [0, 1), for tests. Defaults to Math.random. */
  random?: () => number;
}

export type QueryValue = string | number | boolean | null | undefined;

export interface RequestOptions {
  /** Query string parameters. null/undefined entries are skipped. */
  query?: Record<string, QueryValue>;
  /** JSON request body. Serialized with JSON.stringify. */
  body?: unknown;
  /** Extra request headers. */
  headers?: Record<string, string>;
  /** Abort signal. Aborts are surfaced immediately and never retried. */
  signal?: AbortSignal;
  /**
   * Opt-in flag marking this specific request as safe to retry after a
   * network error even though its verb is non-idempotent (for example a
   * POST create with a client-generated ID, or a check-then-create flow).
   *
   * Background (docs/RATE_LIMITING.md): a 429 is generated by the gateway
   * before proxying, so retrying after a pure 429 can never double-apply a
   * write and is always automatic. A network error is different: the write
   * may already have been applied upstream, so POST and PATCH are not
   * retried on network errors unless this flag is set.
   */
  idempotent?: boolean;
}

/** A successful (2xx) response. */
export interface ApiResponse<T> {
  /** Parsed JSON body, or undefined for empty bodies (204). */
  data: T;
  status: number;
  headers: Headers;
  /** Parsed X-RateLimit-* headers. Watch `remaining` and slow down
   * proactively as it approaches 0. */
  rateLimit: RateLimitInfo;
}

/** Verbs whose network-error retries are safe by default. PATCH is excluded
 * because it is not idempotent in general; use `idempotent: true` per
 * request to opt in. */
const IDEMPOTENT_METHODS = new Set(["GET", "HEAD", "OPTIONS", "PUT", "DELETE"]);

function isAbortLike(error: unknown): boolean {
  return (
    error instanceof Error &&
    (error.name === "AbortError" || error.name === "TimeoutError")
  );
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export class InfraPortalClient {
  private readonly baseUrl: string;
  private token: string | undefined;
  private readonly fetchImpl: typeof globalThis.fetch;
  private readonly retry: RetryOptions;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly random: () => number;

  constructor(options: ClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.token = options.token;
    this.fetchImpl = options.fetch ?? globalThis.fetch;
    this.retry = { ...DEFAULT_RETRY_OPTIONS, ...options.retry };
    this.sleep = options.sleep ?? defaultSleep;
    this.random = options.random ?? Math.random;
    if (typeof this.fetchImpl !== "function") {
      throw new TypeError(
        "No fetch implementation available: pass { fetch } or run on Node 18+.",
      );
    }
  }

  /** Replaces the bearer token (tokens expire; clients are long-lived). */
  setToken(token: string | undefined): void {
    this.token = token;
  }

  /**
   * Sends a request and returns the parsed 2xx response.
   *
   * Retry behavior (docs/RATE_LIMITING.md, in priority order):
   * 1. On 429, honor Retry-After (integer delay-seconds) when parseable.
   * 2. Otherwise sleep a full-jitter backoff: random() * min(maxDelayMs,
   *    baseDelayMs * 2^attempt).
   * 3. Give up after `maxRetries` retries and surface the final error.
   * Only 429 responses and network errors are retried. 429s are retried for
   * every verb (the gateway rejects before proxying, so the upstream never
   * saw the request). Network errors are retried only for idempotent verbs
   * (GET, HEAD, OPTIONS, PUT, DELETE) unless the request opts in with
   * `idempotent: true`. Other statuses (401, 403, 404, 4xx, 5xx) are never
   * retried and throw ApiError.
   */
  async request<T>(
    method: string,
    path: string,
    options: RequestOptions = {},
  ): Promise<ApiResponse<T>> {
    const verb = method.toUpperCase();
    const url = this.buildUrl(path, options.query);
    const headers: Record<string, string> = {
      Accept: "application/json",
      ...options.headers,
    };
    if (this.token !== undefined) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }
    let body: string | undefined;
    if (options.body !== undefined) {
      body = JSON.stringify(options.body);
      headers["Content-Type"] = "application/json";
    }
    const retryOnNetworkError =
      options.idempotent ?? IDEMPOTENT_METHODS.has(verb);

    for (let attempt = 0; ; attempt++) {
      let response: Response;
      try {
        response = await this.fetchImpl(url, {
          method: verb,
          headers,
          body,
          signal: options.signal,
        });
      } catch (error) {
        if (isAbortLike(error)) throw error;
        if (!retryOnNetworkError || attempt >= this.retry.maxRetries) {
          throw error;
        }
        await this.sleep(this.backoffDelayMs(attempt, null));
        continue;
      }

      if (response.status === 429 && attempt < this.retry.maxRetries) {
        const retryAfterSeconds = parseRetryAfterSeconds(response.headers);
        // Consume the body so the connection can be reused.
        await response.text().catch(() => undefined);
        await this.sleep(this.backoffDelayMs(attempt, retryAfterSeconds));
        continue;
      }

      const rateLimit = parseRateLimit(response.headers);
      if (!response.ok) {
        throw await this.toApiError(response, rateLimit);
      }
      const data = await this.parseBody<T>(response);
      return { data, status: response.status, headers: response.headers, rateLimit };
    }
  }

  /** Delay before retry number `attempt + 1`. Retry-After wins when the
   * server sent it; otherwise capped full-jitter exponential backoff. */
  private backoffDelayMs(attempt: number, retryAfterSeconds: number | null): number {
    if (retryAfterSeconds !== null) {
      return retryAfterSeconds * 1000;
    }
    const ceiling = Math.min(
      this.retry.maxDelayMs,
      this.retry.baseDelayMs * 2 ** attempt,
    );
    return this.random() * ceiling;
  }

  private buildUrl(path: string, query?: Record<string, QueryValue>): string {
    const url = this.baseUrl + (path.startsWith("/") ? path : `/${path}`);
    if (!query) return url;
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(query)) {
      if (value === null || value === undefined) continue;
      params.set(key, String(value));
    }
    const qs = params.toString();
    return qs === "" ? url : `${url}?${qs}`;
  }

  private async toApiError(
    response: Response,
    rateLimit: RateLimitInfo,
  ): Promise<ApiError> {
    const retryAfterSeconds = parseRetryAfterSeconds(response.headers);
    let raw = "";
    try {
      raw = await response.text();
    } catch {
      // Body unavailable; fall through to the status-only error.
    }
    let envelope: ApiErrorEnvelope | null = null;
    if (raw !== "") {
      try {
        envelope = parseErrorEnvelope(JSON.parse(raw));
      } catch {
        // Not JSON (for example axum's text/plain rejections).
      }
    }
    const errorArgs: ConstructorParameters<typeof ApiError>[0] = {
      status: response.status,
      code: envelope?.code ?? UNKNOWN_ERROR_CODE,
      message:
        envelope?.message !== undefined && envelope.message !== ""
          ? envelope.message
          : raw !== ""
            ? raw
            : `HTTP ${response.status}`,
      rateLimit,
      retryAfterSeconds,
    };
    if (envelope !== null && "details" in envelope) {
      errorArgs.details = envelope.details;
    }
    return new ApiError(errorArgs);
  }

  private async parseBody<T>(response: Response): Promise<T> {
    if (response.status === 204) {
      return undefined as T;
    }
    const text = await response.text();
    if (text === "") {
      return undefined as T;
    }
    const contentType = response.headers.get("Content-Type") ?? "";
    if (contentType.includes("json")) {
      return JSON.parse(text) as T;
    }
    return text as unknown as T;
  }
}
