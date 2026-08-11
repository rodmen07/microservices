//! First behaviour coverage for the JWT authentication boundary — the check
//! that stands in front of every `/api/v1` route on all eleven services.
//!
//! **The gap, measured on `origin/main` at `19d8a1f` before this suite was
//! written rather than inherited:** `grep -rn validate_authorization_header
//! */tests/*.rs` returned nothing. Eleven services carry a copy of
//! `src/lib/auth.rs` and not one of them had a test that handed the function a
//! header. The two `role_gating.rs` suites (spend PR #136, search PR #137)
//! drive real routes, but every token they mint is well formed and correctly
//! signed and they differ only in the `roles` claim — so they assert
//! AUTHORIZATION and say nothing about AUTHENTICATION. An `exp` that stopped
//! being validated, an issuer check that stopped running, an `alg: none`
//! token, or a token signed with somebody else's secret would leave both of
//! them green.
//!
//! **Two halves, and the second is what makes the first a statement about the
//! platform.** The behaviour tests call accounts-service's own copy of the
//! function. `every_service_shares_one_authorization_boundary` then reads all
//! eleven `*-service/src/lib/auth.rs` files off the disk and proves that the
//! ten functions making up the boundary are, comment-for-comment aside,
//! character-identical everywhere — so the behaviour asserted here is the
//! behaviour of all eleven, and the day one service's copy diverges the guard
//! reddens and names it. The two halves derive from DIFFERENT artifacts (a
//! compiled call into this crate versus text read from ten sibling crates), so
//! no single perturbation moves both in the same direction by accident.
//!
//! **Two `known_gap_` tests, both found by writing this suite, both filed and
//! deliberately not fixed here** (`AUTH-ISS-OPTIONAL-1` and
//! `AUTH-ALG-FALLBACK-1` in `backlogs/microservices.md`). They pin what the
//! code does TODAY rather than what it should do, so the defects have a
//! mechanical existence: fixing either one must redden its test, and that red
//! is the signal to close the backlog entry.
//!
//! **No database, and no service is built but this one.** The boundary rejects
//! long before a handler runs, so every case here is a direct call. The drift
//! guard reads its siblings at RUN time, not with `include_str!`, so a change
//! in another service reddens this suite without rebuilding that service.
//!
//! **Why this suite declares a secret and an issuer the environment cannot
//! supply.** `rust.yml:197` exports `AUTH_JWT_SECRET=dev-insecure-secret-change-me`
//! for `cargo test`, and `AUTH_ISSUER` defaults to `auth-service` when unset.
//! If the suite signed with those values it would still pass with its own
//! `env::set_var` calls deleted — it would be certifying ambient state. Both
//! values below are therefore ones no deployment and no CI job can hold, which
//! makes the green a statement about what the suite declared.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use accounts_service::auth::{validate_authorization_header, AuthClaims, AuthError};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};

/// Signing secret for this suite. Deliberately NOT the value `rust.yml:197`
/// exports for `cargo test`, so a test can only pass because `with_auth_env`
/// declared it (see the module header).
const SUITE_SECRET: &str = "auth-boundary-suite-secret-3f9c1d";

/// Expected issuer for this suite. Deliberately NOT `auth-service`, the value
/// `auth_issuer()` falls back to when `AUTH_ISSUER` is unset.
const SUITE_ISSUER: &str = "auth-boundary-suite-issuer";

/// Some other party's HMAC secret — a token signed with this must never be
/// accepted while the service is configured with `SUITE_SECRET`.
const FOREIGN_SECRET: &str = "auth-boundary-suite-foreign-secret";

/// `exp` far enough ahead that the suite cannot start failing with time.
const FAR_FUTURE_EXP: u64 = 9_999_999_999;

/// `exp` in 2020 — expired by far more than `jsonwebtoken`'s 60 s leeway.
const EXPIRED_EXP: u64 = 1_600_000_000;

/// The eleven workspace services each carry a copy of `src/lib/auth.rs`. A
/// smaller discovery means the layout changed, not that the platform shrank.
const MIN_SERVICES: usize = 11;

// ── Environment isolation ─────────────────────────────────────────────────────

/// `validate_authorization_header` reads four process-wide environment
/// variables on every call, so every call in this binary is serialised behind
/// one lock and every variable is written on every entry. Nothing here depends
/// on test order, and a poisoned lock is recovered rather than cascading.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The service's JWT configuration for one call.
struct AuthEnv {
    /// `AUTH_JWT_ALGORITHM`; `None` removes it, exercising the HS256 default.
    algorithm: Option<&'static str>,
    /// `AUTH_JWT_PUBLIC_KEY`; `None` removes it.
    public_key: Option<&'static str>,
}

impl AuthEnv {
    /// The configuration all eleven services run in production today: neither
    /// `AUTH_JWT_ALGORITHM` nor `AUTH_JWT_PUBLIC_KEY` is set by the deploy
    /// step, so both fall back (HS256, HMAC secret).
    fn deployed_default() -> Self {
        Self {
            algorithm: None,
            public_key: None,
        }
    }
}

/// Sets the whole JWT configuration and validates `header` under it, holding
/// the environment lock across both so concurrent tests cannot interleave.
fn with_auth_env(env_config: AuthEnv, header: Option<&str>) -> Result<AuthClaims, AuthError> {
    let _guard = env_lock();
    env::set_var("AUTH_JWT_SECRET", SUITE_SECRET);
    env::set_var("AUTH_ISSUER", SUITE_ISSUER);
    match env_config.algorithm {
        Some(algorithm) => env::set_var("AUTH_JWT_ALGORITHM", algorithm),
        None => env::remove_var("AUTH_JWT_ALGORITHM"),
    }
    match env_config.public_key {
        Some(key) => env::set_var("AUTH_JWT_PUBLIC_KEY", key),
        None => env::remove_var("AUTH_JWT_PUBLIC_KEY"),
    }
    validate_authorization_header(header)
}

/// Validates `header` under the deployed configuration.
fn validate(header: Option<&str>) -> Result<AuthClaims, AuthError> {
    with_auth_env(AuthEnv::deployed_default(), header)
}

// ── Token minting ─────────────────────────────────────────────────────────────

/// The claim set the platform issues: subject, issuer, expiry, roles.
fn claims_with(roles: Value) -> Value {
    json!({
        "sub": "auth-boundary-subject",
        "iss": SUITE_ISSUER,
        "exp": FAR_FUTURE_EXP,
        "roles": roles,
    })
}

/// Signs `claims` with `secret` under `algorithm`.
fn sign_with(claims: &Value, secret: &str, algorithm: Algorithm) -> String {
    encode(
        &Header::new(algorithm),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("signing the test JWT failed")
}

/// Signs `claims` the way the platform's auth-service does: HS256 with the
/// shared secret this suite declared.
fn sign(claims: &Value) -> String {
    sign_with(claims, SUITE_SECRET, Algorithm::HS256)
}

/// A complete, correctly signed `Authorization` header value.
fn bearer(claims: &Value) -> String {
    format!("Bearer {}", sign(claims))
}

/// Base64url without padding, the JWT segment encoding.
///
/// Hand-rolled because `jsonwebtoken` does not expose its encoder and this
/// suite needs to assemble a token it refuses to sign — an `alg: none` one.
/// `the_encoder_agrees_with_jsonwebtoken` proves it correct against the
/// library's own output rather than against a literal typed from memory.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let triple = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let packed =
            (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);
        let indices = [
            (packed >> 18) & 0x3f,
            (packed >> 12) & 0x3f,
            (packed >> 6) & 0x3f,
            packed & 0x3f,
        ];
        let emit = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in indices.iter().take(emit) {
            out.push(ALPHABET[*index as usize] as char);
        }
    }
    out
}

// ── Header parsing ────────────────────────────────────────────────────────────

#[test]
fn a_request_with_no_authorization_header_is_rejected_as_auth_required() {
    let error = validate(None).expect_err("an absent Authorization header must not authenticate");
    assert!(
        matches!(error, AuthError::MissingHeader),
        "expected MissingHeader for an absent header, got {error:?}"
    );
    assert_eq!(error.code(), "AUTH_REQUIRED");
}

#[test]
fn a_malformed_authorization_header_is_rejected_before_any_signature_check() {
    let token = sign(&claims_with(json!(["admin"])));
    let malformed: Vec<(&str, String)> = vec![
        ("empty header", String::new()),
        ("whitespace only", "   ".to_string()),
        ("no scheme, bare token", token.clone()),
        ("wrong scheme", format!("Basic {token}")),
        ("scheme with no token", "Bearer".to_string()),
        ("a third component", format!("Bearer {token} extra")),
    ];

    for (shape, header) in malformed {
        let Err(error) = validate(Some(&header)) else {
            panic!("[{shape}] must be rejected, but it authenticated");
        };
        assert!(
            matches!(error, AuthError::InvalidHeaderFormat),
            "[{shape}] expected InvalidHeaderFormat, got {error:?}"
        );
        assert_eq!(error.code(), "AUTH_INVALID_FORMAT", "[{shape}] wrong code");
    }
}

#[test]
fn the_bearer_scheme_is_matched_case_insensitively_and_tolerates_extra_whitespace() {
    let token = sign(&claims_with(json!(["admin"])));
    for header in [
        format!("bearer {token}"),
        format!("BEARER {token}"),
        format!("Bearer   {token}"),
        format!("Bearer\t{token}"),
    ] {
        let claims = validate(Some(&header))
            .unwrap_or_else(|err| panic!("{header:?} should authenticate, got {err:?}"));
        assert_eq!(claims.sub, "auth-boundary-subject");
    }
}

// ── Token validation ──────────────────────────────────────────────────────────

#[test]
fn a_well_formed_token_is_accepted_and_its_claims_are_read() {
    let claims = validate(Some(&bearer(&claims_with(json!(["admin", "client"])))))
        .expect("a correctly signed, unexpired token from the configured issuer must authenticate");

    assert_eq!(claims.sub, "auth-boundary-subject");
    assert_eq!(
        claims.roles,
        vec!["admin".to_string(), "client".to_string()]
    );
    assert!(claims.has_role("admin"));
    assert!(!claims.has_role("service"));
}

#[test]
fn an_expired_token_is_rejected() {
    let mut claims = claims_with(json!(["admin"]));
    claims["exp"] = json!(EXPIRED_EXP);

    let error = validate(Some(&bearer(&claims)))
        .expect_err("a token whose exp is years in the past must not authenticate");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for an expired token, got {error:?}"
    );
    assert_eq!(error.code(), "AUTH_INVALID_TOKEN");
}

#[test]
fn a_token_carrying_no_exp_claim_is_rejected() {
    let claims = json!({
        "sub": "auth-boundary-subject",
        "iss": SUITE_ISSUER,
        "roles": ["admin"],
    });

    let error = validate(Some(&bearer(&claims)))
        .expect_err("a token with no expiry at all must not authenticate");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for a token with no exp claim, got {error:?}"
    );
}

#[test]
fn a_token_declaring_another_issuer_is_rejected() {
    let mut claims = claims_with(json!(["admin"]));
    claims["iss"] = json!("some-other-auth-service");

    let error = validate(Some(&bearer(&claims)))
        .expect_err("a token minted by a different issuer must not authenticate");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for a foreign issuer, got {error:?}"
    );
}

#[test]
fn known_gap_a_token_with_no_issuer_claim_at_all_is_accepted() {
    // Characterisation test for AUTH-ISS-OPTIONAL-1 (MED — filed in
    // backlogs/microservices.md, deliberately NOT fixed here: making `iss`
    // mandatory would start rejecting every live token that omits it, which is
    // a production behaviour change and its own increment).
    //
    // This is a PUBLISHED contract the code does not keep. `docs/API.md:42`
    // and the `bearerAuth` description in all eleven `openapi.yaml` files —
    // rendered on the public playground — say "Tokens must carry `exp` and an
    // `iss` matching AUTH_ISSUER (default \"auth-service\")". The `exp` half
    // holds (`a_token_carrying_no_exp_claim_is_rejected`); the `iss` half does
    // not. The doc states the stronger, intended contract, so the fix belongs
    // in the code and the doc must NOT be weakened to match this test.
    //
    // `validate_authorization_header` calls `validation.set_issuer(...)`, but
    // that only populates the SET of acceptable issuers; it does not add `iss`
    // to `required_spec_claims` (which `Validation::new` leaves as `{"exp"}`).
    // So the issuer check is one-directional: it refuses a token honest enough
    // to declare a foreign issuer, and admits one that declares none.
    // `a_token_declaring_another_issuer_is_rejected` is the half that holds.
    //
    // This test PINS TODAY'S WRONG BEHAVIOUR. When `iss` becomes mandatory it
    // must go red; that red is the signal to close the backlog entry.
    let mut claims = claims_with(json!(["admin"]));
    claims
        .as_object_mut()
        .expect("claims are a JSON object")
        .remove("iss");

    let outcome = validate(Some(&bearer(&claims)));
    assert!(
        outcome.is_ok(),
        "GAP-CLOSED: a token carrying no `iss` claim is now refused, so the \
         one-directional issuer check AUTH-ISS-OPTIONAL-1 describes is fixed. \
         Close that entry in backlogs/microservices.md and fold this case back \
         into a_token_declaring_another_issuer_is_rejected. (Observed: {outcome:?})"
    );
}

#[test]
fn a_token_signed_with_another_secret_is_rejected() {
    let claims = claims_with(json!(["admin"]));
    let foreign = sign_with(&claims, FOREIGN_SECRET, Algorithm::HS256);

    let error = validate(Some(&format!("Bearer {foreign}")))
        .expect_err("a token signed with a secret this service does not hold must be rejected");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for a foreign signature, got {error:?}"
    );
}

#[test]
fn the_encoder_agrees_with_jsonwebtoken() {
    // The `alg: none` probe below is assembled by hand, so the encoder that
    // assembles it is proven against the library's own segment encoding first.
    let claims = claims_with(json!(["admin"]));
    let token = sign(&claims);
    let payload_segment = token
        .split('.')
        .nth(1)
        .expect("a signed JWT has three dot-separated segments");
    let encoded = base64url(&serde_json::to_vec(&claims).expect("claims serialise"));

    assert_eq!(
        encoded, payload_segment,
        "the suite's base64url encoder disagrees with jsonwebtoken's, so the \
         alg:none probe it builds cannot be trusted"
    );
}

#[test]
fn an_unsigned_alg_none_token_is_rejected() {
    let claims = claims_with(json!(["admin"]));
    let header = base64url(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64url(&serde_json::to_vec(&claims).expect("claims serialise"));
    // The classic algorithm-confusion probe: a well-formed token whose header
    // declares no signature at all, with an empty signature segment.
    let unsigned = format!("{header}.{payload}.");

    let error = validate(Some(&format!("Bearer {unsigned}")))
        .expect_err("an unsigned alg:none token must never authenticate");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for an alg:none token, got {error:?}"
    );
}

#[test]
fn a_token_signed_under_a_different_algorithm_is_rejected() {
    let claims = claims_with(json!(["admin"]));
    // Same secret, same claims — only the declared algorithm differs, so the
    // rejection can only come from the algorithm pin in `Validation`.
    let hs512 = sign_with(&claims, SUITE_SECRET, Algorithm::HS512);

    let error = validate(Some(&format!("Bearer {hs512}")))
        .expect_err("a token signed under an algorithm the service does not accept is invalid");
    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken for an HS512 token under an HS256 configuration, got {error:?}"
    );
}

#[test]
fn a_token_whose_claims_do_not_fit_the_contract_is_rejected_outright() {
    let mut roles_as_string = claims_with(json!(["admin"]));
    roles_as_string["roles"] = json!("admin");

    let mut no_subject = claims_with(json!(["admin"]));
    no_subject
        .as_object_mut()
        .expect("claims are a JSON object")
        .remove("sub");

    // Both must FAIL CLOSED rather than degrade into an anonymous-but-valid
    // identity: a `roles` string silently read as "no roles" would turn every
    // role gate into a 403 instead of a 401, and a subject-less token would
    // authenticate a request nothing can be attributed to.
    for (shape, claims) in [
        ("roles is a string, not an array", roles_as_string),
        ("no sub claim", no_subject),
    ] {
        let Err(error) = validate(Some(&bearer(&claims))) else {
            panic!("[{shape}] must be rejected, but it authenticated");
        };
        assert!(
            matches!(error, AuthError::InvalidToken),
            "[{shape}] expected InvalidToken, got {error:?}"
        );
    }
}

#[test]
fn a_token_with_no_roles_claim_authenticates_with_no_roles() {
    let claims = json!({
        "sub": "auth-boundary-subject",
        "iss": SUITE_ISSUER,
        "exp": FAR_FUTURE_EXP,
    });

    let decoded = validate(Some(&bearer(&claims)))
        .expect("a roles-less token is authentic; it is authorisation that must refuse it");
    assert!(
        decoded.roles.is_empty(),
        "expected no roles, got {:?}",
        decoded.roles
    );
    assert!(!decoded.has_role("admin"));
}

#[test]
fn the_roles_claim_is_matched_case_insensitively() {
    let claims = validate(Some(&bearer(&claims_with(json!(["ADMIN"])))))
        .expect("a correctly signed token authenticates whatever case its roles use");

    assert!(
        claims.has_role("admin"),
        "has_role is documented case-insensitive, but ADMIN did not match admin"
    );
}

#[test]
fn an_rs256_configuration_without_a_public_key_fails_closed() {
    // With AUTH_JWT_ALGORITHM=RS256 and no AUTH_JWT_PUBLIC_KEY, `decoding_key`
    // has nothing to verify against. The only safe answer is refusal — and in
    // particular it must NOT fall back to the HMAC secret, which every service
    // holds.
    let hmac_token = bearer(&claims_with(json!(["admin"])));
    let error = with_auth_env(
        AuthEnv {
            algorithm: Some("RS256"),
            public_key: None,
        },
        Some(&hmac_token),
    )
    .expect_err("an RS256 service with no public key must reject every token");

    assert!(
        matches!(error, AuthError::InvalidToken),
        "expected InvalidToken when the RSA key is missing, got {error:?}"
    );
}

#[test]
fn known_gap_an_unrecognised_jwt_algorithm_falls_back_to_hs256() {
    // Characterisation test for AUTH-ALG-FALLBACK-1 (LOW, latent — filed in
    // backlogs/microservices.md, deliberately NOT fixed here). `auth_algorithm`
    // ends in `_ => Algorithm::HS256`, so any value it does not recognise —
    // including a typo of an RS* name — silently downgrades the boundary from
    // "only the auth-service's private key can mint tokens" to "anyone holding
    // the shared HMAC secret can", with the configured public key ignored.
    //
    // This test PINS TODAY'S WRONG BEHAVIOUR. When the fallback is fixed it
    // must go red; that red is the signal to close the backlog entry.
    let hmac_token = bearer(&claims_with(json!(["admin"])));
    let outcome = with_auth_env(
        AuthEnv {
            // A plausible operator typo for RS256.
            algorithm: Some("RS2566"),
            // A public key is configured, and is about to be ignored.
            public_key: Some(
                "-----BEGIN PUBLIC KEY-----\\nnot-a-real-key\\n-----END PUBLIC KEY-----",
            ),
        },
        Some(&hmac_token),
    );

    assert!(
        outcome.is_ok(),
        "GAP-CLOSED: an unrecognised AUTH_JWT_ALGORITHM no longer falls back to \
         HS256 — the shared-secret downgrade AUTH-ALG-FALLBACK-1 describes is \
         fixed. Close that entry in backlogs/microservices.md and invert this \
         test into the assertion that the misconfiguration is refused. \
         (Observed: {outcome:?})"
    );
}

// ── The boundary is one boundary: drift guard over all eleven services ────────

/// Every function that makes up the authorisation-header boundary. A service
/// missing one, or holding a different version of one, is drift.
const GUARDED_FUNCTIONS: &[&str] = &[
    "has_role",
    "code",
    "message",
    "auth_secret",
    "auth_algorithm",
    "auth_issuer",
    "normalise_pem",
    "decoding_key",
    "extract_bearer_token",
    "validate_authorization_header",
];

/// Reads every `*-service/src/lib/auth.rs` in the workspace, discovered by
/// walking the workspace root rather than hand-listed, so a service added or
/// renamed joins the guard with no edit here.
fn discover_auth_sources() -> Vec<(String, String)> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("accounts-service must sit inside the workspace root")
        .to_path_buf();

    let entries = fs::read_dir(&workspace_root)
        .unwrap_or_else(|err| panic!("CANNOT-READ: workspace root {workspace_root:?}: {err}"));

    let mut sources = Vec::new();
    for entry in entries {
        let entry = entry.expect("read workspace dir entry");
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        if !dir_name.ends_with("-service") {
            continue;
        }
        let auth_rs = entry.path().join("src").join("lib").join("auth.rs");
        if !auth_rs.is_file() {
            continue;
        }
        // A file that exists and cannot be read is UNVERIFIED, never clean.
        let source = fs::read_to_string(&auth_rs)
            .unwrap_or_else(|err| panic!("CANNOT-READ: {dir_name}/src/lib/auth.rs: {err}"));
        assert!(
            !source.trim().is_empty(),
            "CANNOT-READ: {dir_name}/src/lib/auth.rs is empty"
        );
        sources.push((dir_name, source));
    }
    sources.sort();
    sources
}

/// Removes comments and collapses whitespace runs, so two copies that differ
/// only in their comments (projects-service carries none; spend and search
/// carry extra role constants) compare equal on the code itself.
///
/// This is a small lexer rather than a regex: it tracks line comments, block
/// comments and string literals, so a `//` inside a string survives and a `{`
/// inside a comment cannot corrupt the brace matching that follows.
fn strip_comments_and_collapse(source: &str) -> String {
    // Two constructs would defeat the lexer. Neither appears in any copy of
    // auth.rs today, and if one ever does the guard REFUSES rather than
    // quietly comparing garbage.
    assert!(
        !source.contains("r#\""),
        "CANNOT-READ: auth.rs now contains a raw string literal; this guard's \
         lexer does not model them. Teach it, do not delete it."
    );
    for forbidden in ["'{'", "'}'", "'\"'"] {
        assert!(
            !source.contains(forbidden),
            "CANNOT-READ: auth.rs now contains the char literal {forbidden}, which \
             this guard's lexer would misread. Teach it, do not delete it."
        );
    }

    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for skipped in chars.by_ref() {
                    if previous == '*' && skipped == '/' {
                        break;
                    }
                    previous = skipped;
                }
                out.push(' ');
            }
            '"' => {
                in_string = true;
                out.push(c);
            }
            c if c.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

/// Returns the signature and body of `fn <name>` from already-normalised
/// source, matched by brace counting with string literals skipped.
fn function_text(normalised: &str, name: &str) -> Option<String> {
    let needle = format!("fn {name}(");
    let occurrences = normalised.matches(&needle).count();
    assert!(
        occurrences <= 1,
        "`{needle}` appears {occurrences} times in one auth.rs; this guard \
         compares the first occurrence only and would silently ignore the rest"
    );
    let start = normalised.find(&needle)?;
    let open = normalised[start..].find('{')? + start;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, c) in normalised[open..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(normalised[start..open + offset + c.len_utf8()].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn every_service_shares_one_authorization_boundary() {
    let sources = discover_auth_sources();
    assert!(
        sources.len() >= MIN_SERVICES,
        "FATAL: discovered {} services carrying src/lib/auth.rs, expected at \
         least {MIN_SERVICES} — refusing to report a verdict on a corpus this \
         small. The workspace layout changed; fix this guard, do not delete it.",
        sources.len()
    );

    let normalised: Vec<(String, String)> = sources
        .iter()
        .map(|(service, source)| (service.clone(), strip_comments_and_collapse(source)))
        .collect();

    for function in GUARDED_FUNCTIONS {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (service, source) in &normalised {
            let text = function_text(source, function).unwrap_or_else(|| {
                panic!(
                    "{service}/src/lib/auth.rs declares no `fn {function}`, so this \
                     service no longer shares the platform's authorisation \
                     boundary. If that is deliberate, say so here."
                )
            });
            groups.entry(text).or_default().push(service.clone());
        }

        assert_eq!(
            groups.len(),
            1,
            "`fn {function}` has drifted: the {} services split into {} different \
             implementations.\n{}",
            normalised.len(),
            groups.len(),
            groups
                .iter()
                .enumerate()
                .map(|(index, (text, services))| format!(
                    "  variant {}: {}\n    {}",
                    index + 1,
                    services.join(", "),
                    text
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
