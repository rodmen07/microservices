/** Shared test helpers: a scripted fetch fake and response builders.
 * No network access anywhere in the test suite. */

/**
 * Builds a fetch fake that replays `steps` in order. Each step is either
 * { error } (the fetch call rejects) or { response: () => Response }.
 * Records every call's url and init for assertions.
 */
export function fakeFetch(steps) {
  const calls = [];
  const impl = async (url, init) => {
    calls.push({ url, init });
    const step = steps.shift();
    if (!step) {
      throw new Error(`fakeFetch: unexpected call #${calls.length} to ${url}`);
    }
    if (step.error) throw step.error;
    return step.response();
  };
  return { impl, calls };
}

export function jsonResponse(status, body, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ...headers },
  });
}

export function textResponse(status, body, headers = {}) {
  return new Response(body, {
    status,
    headers: { "Content-Type": "text/plain", ...headers },
  });
}

export function rateLimited429(headers = {}) {
  return jsonResponse(
    429,
    { code: "RATE_LIMITED", message: "rate limit exceeded (30 req/s), retry after 1s" },
    headers,
  );
}

/** Records sleep durations instead of waiting. */
export function fakeSleep() {
  const sleeps = [];
  const sleep = async (ms) => {
    sleeps.push(ms);
  };
  return { sleep, sleeps };
}
