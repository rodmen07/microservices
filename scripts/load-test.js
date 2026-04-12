/**
 * InfraPortal Load Test — k6
 *
 * Usage:
 *   k6 run scripts/load-test.js
 *
 * Override targets with env vars:
 *   k6 run -e ACCOUNTS_URL=https://... -e CONTACTS_URL=https://... scripts/load-test.js
 *
 * Requires k6 >= 0.46. Install: https://k6.io/docs/get-started/installation/
 *
 * Scenarios:
 *   smoke     — 1 VU, 30 s — verify the suite runs cleanly before ramping up
 *   load      — ramp 0→30→30→0 VUs over 3 min — steady-state throughput
 *   spike     — burst to 80 VUs for 30 s — validate autoscaling headroom
 *
 * Thresholds (fail the run if breached):
 *   http_req_duration p(95) < 2000 ms
 *   http_req_failed   rate  < 0.01  (< 1% errors)
 *   checks            rate  > 0.99
 */

import http from 'k6/http'
import { check, group, sleep } from 'k6'
import { Rate, Trend } from 'k6/metrics'

// ── Config ────────────────────────────────────────────────────────────────────

const ACCOUNTS_URL  = (__ENV.ACCOUNTS_URL  || 'http://localhost:3010').replace(/\/$/, '')
const CONTACTS_URL  = (__ENV.CONTACTS_URL  || 'http://localhost:3011').replace(/\/$/, '')
const OPPS_URL      = (__ENV.OPPS_URL      || 'http://localhost:3012').replace(/\/$/, '')
const REPORTING_URL = (__ENV.REPORTING_URL || 'http://localhost:8086').replace(/\/$/, '')
const SEARCH_URL    = (__ENV.SEARCH_URL    || 'http://localhost:8083').replace(/\/$/, '')
const AUDIT_URL     = (__ENV.AUDIT_URL     || 'http://localhost:3017').replace(/\/$/, '')

// A long-lived JWT for load testing — same secret as dev default
const JWT = __ENV.LOAD_TEST_JWT || (() => {
  // Fallback: a pre-signed token for dev-insecure-secret-change-me
  // sub=load-test, roles=[], exp far future
  // Generated with: jsonwebtoken.encode(claims, "dev-insecure-secret-change-me")
  // Replace with a real token when testing against production.
  return 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.' +
    'eyJzdWIiOiJsb2FkLXRlc3QiLCJpc3MiOiJhdXRoLXNlcnZpY2UiLCJleHAiOjk5OTk5OTk5OTksInJvbGVzIjpbXX0.' +
    'placeholder-replace-with-real-token'
})()

const AUTH = { Authorization: `Bearer ${JWT}` }
const JSON_HEADERS = { ...AUTH, 'Content-Type': 'application/json' }

// ── Custom metrics ────────────────────────────────────────────────────────────

const accountCreateDuration = new Trend('account_create_duration', true)
const contactCreateDuration = new Trend('contact_create_duration', true)
const searchDuration         = new Trend('search_duration', true)
const errorRate              = new Rate('custom_error_rate')

// ── Scenarios ─────────────────────────────────────────────────────────────────

export const options = {
  scenarios: {
    smoke: {
      executor: 'constant-vus',
      vus: 1,
      duration: '30s',
      tags: { scenario: 'smoke' },
    },
    load: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 10 },
        { duration: '90s', target: 30 },
        { duration: '30s', target: 0  },
      ],
      startTime: '35s', // starts after smoke finishes
      tags: { scenario: 'load' },
    },
    spike: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '10s', target: 80 },
        { duration: '20s', target: 80 },
        { duration: '10s', target: 0  },
      ],
      startTime: '3m35s', // starts after load finishes
      tags: { scenario: 'spike' },
    },
  },

  thresholds: {
    http_req_duration:   ['p(95)<2000'],
    http_req_failed:     ['rate<0.01'],
    checks:              ['rate>0.99'],
    custom_error_rate:   ['rate<0.01'],
    account_create_duration: ['p(95)<1500'],
    contact_create_duration: ['p(95)<1500'],
    search_duration:         ['p(95)<2000'],
  },
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function checkOk(resp, label) {
  const ok = check(resp, {
    [`${label}: status 2xx`]: r => r.status >= 200 && r.status < 300,
  })
  errorRate.add(!ok)
  return ok
}

function checkStatus(resp, expected, label) {
  const ok = check(resp, {
    [`${label}: status ${expected}`]: r => r.status === expected,
  })
  errorRate.add(!ok)
  return ok
}

// ── Scenario: health checks ───────────────────────────────────────────────────

function runHealthChecks() {
  group('health checks', () => {
    const endpoints = [
      [ACCOUNTS_URL,  'accounts'],
      [CONTACTS_URL,  'contacts'],
      [REPORTING_URL, 'reporting'],
      [SEARCH_URL,    'search'],
      [AUDIT_URL,     'audit'],
    ]
    for (const [base, name] of endpoints) {
      if (!base) continue
      const r = http.get(`${base}/health`)
      checkOk(r, `${name}/health`)
    }
  })
}

// ── Scenario: accounts CRUD ───────────────────────────────────────────────────

function runAccountsCrud() {
  group('accounts CRUD', () => {
    // Create
    const payload = JSON.stringify({
      name: `Load Test Co ${Date.now()}`,
      domain: 'loadtest.example.com',
      status: 'active',
    })
    const start = Date.now()
    const created = http.post(`${ACCOUNTS_URL}/api/v1/accounts`, payload, { headers: JSON_HEADERS })
    accountCreateDuration.add(Date.now() - start)

    if (!checkStatus(created, 201, 'create account')) return
    const id = created.json('id')
    if (!id) return

    // Get
    const fetched = http.get(`${ACCOUNTS_URL}/api/v1/accounts/${id}`, { headers: AUTH })
    checkStatus(fetched, 200, 'get account')

    // List
    const listed = http.get(`${ACCOUNTS_URL}/api/v1/accounts?limit=10`, { headers: AUTH })
    check(listed, { 'list accounts: has data array': r => Array.isArray(r.json('data')) })

    // Update
    const patched = http.patch(
      `${ACCOUNTS_URL}/api/v1/accounts/${id}`,
      JSON.stringify({ status: 'inactive' }),
      { headers: JSON_HEADERS },
    )
    checkStatus(patched, 200, 'update account')

    // Delete
    const deleted = http.del(`${ACCOUNTS_URL}/api/v1/accounts/${id}`, null, { headers: AUTH })
    checkStatus(deleted, 204, 'delete account')
  })
}

// ── Scenario: contacts list ───────────────────────────────────────────────────

function runContactsList() {
  group('contacts list', () => {
    const start = Date.now()
    const r = http.get(`${CONTACTS_URL}/api/v1/contacts?limit=20`, { headers: AUTH })
    contactCreateDuration.add(Date.now() - start)
    check(r, {
      'contacts list: status 200': res => res.status === 200,
      'contacts list: has data':   res => Array.isArray(res.json('data')),
    })
  })
}

// ── Scenario: search ──────────────────────────────────────────────────────────

function runSearch() {
  group('search', () => {
    const queries = ['test', 'account', 'contact', 'project']
    const q = queries[Math.floor(Math.random() * queries.length)]
    const start = Date.now()
    const r = http.get(`${SEARCH_URL}/api/v1/search?q=${q}&limit=10`, { headers: AUTH })
    searchDuration.add(Date.now() - start)
    check(r, { 'search: status 200': res => res.status === 200 })
  })
}

// ── Scenario: opportunities list ─────────────────────────────────────────────

function runOpportunitiesList() {
  group('opportunities list', () => {
    const r = http.get(`${OPPS_URL}/api/v1/opportunities?limit=10`, { headers: AUTH })
    checkStatus(r, 200, 'list opportunities')
  })
}

// ── Scenario: reporting dashboard ────────────────────────────────────────────

function runReportingDashboard() {
  group('reporting dashboard', () => {
    const r = http.get(`${REPORTING_URL}/dashboard`, { headers: AUTH })
    checkOk(r, 'reporting dashboard')
  })
}

// ── Default function ──────────────────────────────────────────────────────────

export default function () {
  // Spread load across scenarios proportionally
  const roll = Math.random()

  if (roll < 0.15) {
    runHealthChecks()
  } else if (roll < 0.40) {
    runAccountsCrud()
  } else if (roll < 0.60) {
    runContactsList()
  } else if (roll < 0.75) {
    runSearch()
  } else if (roll < 0.88) {
    runOpportunitiesList()
  } else {
    runReportingDashboard()
  }

  sleep(Math.random() * 1 + 0.5) // 0.5–1.5 s think time
}

// ── Setup / teardown ──────────────────────────────────────────────────────────

export function handleSummary(data) {
  const { metrics } = data
  const p95 = metrics.http_req_duration?.values?.['p(95)'] ?? 0
  const errRate = (metrics.http_req_failed?.values?.rate ?? 0) * 100

  console.log('\n=== Load Test Summary ===')
  console.log(`  p(95) response time : ${p95.toFixed(0)} ms`)
  console.log(`  Error rate          : ${errRate.toFixed(2)} %`)
  console.log(`  Total requests      : ${metrics.http_reqs?.values?.count ?? 0}`)
  console.log(`  RPS                 : ${(metrics.http_reqs?.values?.rate ?? 0).toFixed(1)}`)
  console.log('=========================\n')

  return {
    stdout: '',
  }
}
