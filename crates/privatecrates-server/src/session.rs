//! Signing in to the website with GitHub, and the session cookie (docs/website-api.md).
//!
//! Sign-in is the reader App's web flow. The resulting user token is the whole session: it is sealed with
//! AES-256-GCM into an `HttpOnly` cookie, so nothing is stored server-side, and it is never logged.

use std::sync::Arc;

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Generate, Nonce, Payload},
};
use axum::{
    extract::{FromRequestParts, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{AppState, auth::Credential, config::Config, error::ApiError, github::now_secs};

pub const SESSION_COOKIE: &str = "pc_session";
/// Binds a sign-in to the browser that started it, so nobody can sign a victim in as themselves.
const SIGN_IN_COOKIE: &str = "pc_sign_in";
const SIGN_IN_PATH: &str = "/auth/github";
const SIGN_IN_TTL_SECS: u64 = 10 * 60;
/// GitHub App user tokens expire after 8 hours; the session never outlives its token.
const MAX_SESSION_SECS: u64 = 8 * 60 * 60;
const DEFAULT_RETURN_TO: &str = "/account";
const NONCE_LEN: usize = 12;

/// Seals and opens the session cookie and the sign-in `state`. Each use passes its own purpose as associated
/// data, so a value sealed for one can never be accepted as the other.
pub struct Sealer {
    cipher: Aes256Gcm,
}

impl Sealer {
    pub fn new(secret: &[u8]) -> Self {
        // SHA-256 turns a secret of any length (at least 32 bytes, checked in config) into a 256-bit key.
        let key = Sha256::digest(secret);
        Self {
            cipher: Aes256Gcm::new_from_slice(&key).expect("SHA-256 output is a 256-bit key"),
        }
    }

    fn seal(&self, purpose: &str, plaintext: &[u8]) -> String {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: purpose.as_bytes(),
                },
            )
            .expect("AES-GCM encrypts messages of this size");
        let mut sealed = nonce.to_vec();
        sealed.extend(ciphertext);
        URL_SAFE_NO_PAD.encode(sealed)
    }

    fn open(&self, purpose: &str, sealed: &str) -> Option<Vec<u8>> {
        let bytes = URL_SAFE_NO_PAD.decode(sealed).ok()?;
        if bytes.len() < NONCE_LEN {
            return None;
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        self.cipher
            .decrypt(
                &Nonce::<Aes256Gcm>::try_from(nonce).ok()?,
                Payload {
                    msg: ciphertext,
                    aad: purpose.as_bytes(),
                },
            )
            .ok()
    }
}

#[derive(Serialize, Deserialize)]
struct SessionCookie {
    token: String,
    expires_at: u64,
}

/// What the sign-in `state` carries through GitHub and back.
#[derive(Serialize, Deserialize)]
struct SignIn {
    return_to: String,
    nonce: String,
    expires_at: u64,
}

/// A signed-in website user, or a tool such as `cargo privatecrates` holding a reader App user token.
pub struct Session {
    /// The user's reader App token. Used only for GitHub calls on their behalf.
    pub token: String,
    /// Whether the token came in an `Authorization` header rather than the session cookie.
    pub bearer: bool,
}

impl Session {
    /// The request's session: from `Authorization: Bearer ghu_…` when the header is present (never falling back to
    /// the cookie then), otherwise from the session cookie. `None` when signed out.
    pub fn from_headers(state: &AppState, headers: &HeaderMap) -> Result<Option<Self>, ApiError> {
        if headers.contains_key(header::AUTHORIZATION) {
            // Only reader App user tokens, as the device flow gives: other GitHub tokens are usually far broader
            // than we need, and we do not want them sent to us.
            return match Credential::from_headers(headers) {
                Some(Credential::AppUser(token)) => Ok(Some(Self {
                    token,
                    bearer: true,
                })),
                _ => Err(ApiError::AccountTokenNotAccepted),
            };
        }
        Ok(cookies(headers, SESSION_COOKIE).find_map(|value| {
            let plaintext = state.sealer.open(SESSION_COOKIE, value)?;
            let cookie: SessionCookie = serde_json::from_slice(&plaintext).ok()?;
            (cookie.expires_at > now_secs()).then_some(Self {
                token: cookie.token,
                bearer: false,
            })
        }))
    }
}

impl FromRequestParts<Arc<AppState>> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Self::from_headers(state, &parts.headers)?.ok_or(ApiError::SignInRequired)
    }
}

/// The values of every cookie called `name`.
fn cookies<'a>(headers: &'a HeaderMap, name: &'a str) -> impl Iterator<Item = &'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(move |pair| {
            let (n, value) = pair.trim().split_once('=')?;
            (n == name).then_some(value)
        })
}

fn cookie(name: &str, value: &str, path: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path={path}; Max-Age={max_age}; HttpOnly; Secure; SameSite=Lax"
    ))
    .expect("cookie values are base64url or hex")
}

pub fn clear_session_cookie() -> HeaderValue {
    cookie(SESSION_COOKIE, "", "/", 0)
}

/// A path on the website, and nothing that a browser could read as another site (`//evil.example`,
/// `/\evil.example`) or that cannot be sent in a `Location` header.
fn is_same_site_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 1024
        && path.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
}

fn callback_url(config: &Config) -> String {
    format!("{}{SIGN_IN_PATH}/callback", config.apex_url())
}

#[derive(Deserialize)]
pub struct LoginQuery {
    return_to: Option<String>,
}

/// Starts signing in: redirects to GitHub's authorisation page for the reader App.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Query(query): Query<LoginQuery>,
) -> Response {
    let return_to = query
        .return_to
        .filter(|r| is_same_site_path(r))
        .unwrap_or_else(|| DEFAULT_RETURN_TO.into());
    let nonce = hex::encode(<[u8; 16]>::generate());
    let sign_in = SignIn {
        return_to,
        nonce: nonce.clone(),
        expires_at: now_secs() + SIGN_IN_TTL_SECS,
    };
    let sealed = state.sealer.seal(
        SIGN_IN_COOKIE,
        &serde_json::to_vec(&sign_in).expect("serialisable"),
    );
    let mut url = state
        .config
        .github_web
        .join("login/oauth/authorize")
        .expect("a valid URL path");
    url.query_pairs_mut()
        .append_pair("client_id", &state.config.reader_client_id)
        .append_pair("redirect_uri", &callback_url(&state.config))
        .append_pair("state", &sealed);
    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_str(url.as_str()).expect("a URL"),
            ),
            (
                header::SET_COOKIE,
                cookie(SIGN_IN_COOKIE, &nonce, SIGN_IN_PATH, SIGN_IN_TTL_SECS),
            ),
        ],
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

/// Finishes signing in: exchanges GitHub's code for a user token and seals it into the session cookie.
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let sign_in = query
        .state
        .as_deref()
        .and_then(|s| state.sealer.open(SIGN_IN_COOKIE, s))
        .and_then(|plaintext| serde_json::from_slice::<SignIn>(&plaintext).ok())
        .filter(|s| s.expires_at > now_secs())
        .ok_or(ApiError::SignInStateInvalid)?;
    let same_browser = cookies(&headers, SIGN_IN_COOKIE)
        .any(|nonce| bool::from(nonce.as_bytes().ct_eq(sign_in.nonce.as_bytes())));
    if !same_browser {
        return Err(ApiError::SignInStateInvalid);
    }
    // Without a code, the user declined on GitHub.
    let code = query.code.ok_or(ApiError::SignInFailed)?;
    let token = state
        .gh
        .exchange_code(
            &state.config.reader_client_id,
            &state.config.reader_client_secret,
            &code,
            &callback_url(&state.config),
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "sign-in code exchange failed");
            ApiError::SignInFailed
        })?;
    let lifetime = token
        .expires_in
        .unwrap_or(MAX_SESSION_SECS)
        .min(MAX_SESSION_SECS);
    let session = SessionCookie {
        token: token.access_token,
        expires_at: now_secs() + lifetime,
    };
    let sealed = state.sealer.seal(
        SESSION_COOKIE,
        &serde_json::to_vec(&session).expect("serialisable"),
    );
    let mut response = StatusCode::FOUND.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(&sign_in.return_to).expect("checked by is_same_site_path"),
    );
    headers.append(
        header::SET_COOKIE,
        cookie(SESSION_COOKIE, &sealed, "/", lifetime),
    );
    headers.append(
        header::SET_COOKIE,
        cookie(SIGN_IN_COOKIE, "", SIGN_IN_PATH, 0),
    );
    Ok(response)
}

pub async fn logout() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_session_cookie())],
    )
        .into_response()
}

/// `POST /auth/logout-everywhere`: signs the user out of PrivateCrates on every device, for a lost or stolen laptop.
/// GitHub revokes the reader App's grant for this user, which ends every token it issued them: the website's
/// sessions, and the credential provider's tokens and refresh tokens on every machine. Nobody else is affected.
pub async fn logout_everywhere(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Response, ApiError> {
    let user = state.gh.user(&session.token).await?;
    state
        .gh
        .revoke_grant(
            &state.config.reader_client_id,
            &state.config.reader_client_secret,
            &session.token,
        )
        .await?;
    // GitHub's webhook says the same, but this request should not depend on it arriving.
    state.permissions.forget_user(user.id, None).await;
    tracing::info!(user = %user.login, "signed out of every device");
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_session_cookie())],
    )
        .into_response())
}

/// CSRF protection for the account API: a state-changing request must be JSON from the website itself. A form on
/// another site can send neither that content type nor our `Origin`.
///
/// Requests with an `Authorization` header are exempt. A bearer token is not an ambient credential: a browser never
/// adds one by itself, and another site cannot set the header without a CORS preflight, which we never grant. Such
/// a request is authenticated by that header alone ([`Session::from_headers`] never falls back to the cookie).
pub async fn same_origin(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if !request.method().is_safe() && !request.headers().contains_key(header::AUTHORIZATION) {
        let headers = request.headers();
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"));
        let origin = headers
            .get(header::ORIGIN)
            .is_some_and(|v| v.as_bytes() == state.config.apex_url().as_bytes());
        if !json || !origin {
            return ApiError::CrossSiteRequest.into_response();
        }
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_values_are_bound_to_their_purpose() {
        let sealer = Sealer::new(&[1; 32]);
        let sealed = sealer.seal(SESSION_COOKIE, b"hello");
        assert_eq!(sealer.open(SESSION_COOKIE, &sealed).unwrap(), b"hello");
        assert!(sealer.open(SIGN_IN_COOKIE, &sealed).is_none());
        assert!(
            Sealer::new(&[2; 32])
                .open(SESSION_COOKIE, &sealed)
                .is_none()
        );
        let mut tampered = URL_SAFE_NO_PAD.decode(&sealed).unwrap();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(
            sealer
                .open(SESSION_COOKIE, &URL_SAFE_NO_PAD.encode(tampered))
                .is_none()
        );
        assert!(sealer.open(SESSION_COOKIE, "short").is_none());
    }

    #[test]
    fn return_to_must_be_a_path_on_this_site() {
        assert!(is_same_site_path("/account"));
        assert!(is_same_site_path("/account?org=acme"));
        assert!(!is_same_site_path("https://evil.example/"));
        assert!(!is_same_site_path("//evil.example/"));
        assert!(!is_same_site_path("/\\evil.example/"));
        assert!(!is_same_site_path("account"));
        assert!(!is_same_site_path("/a b"));
        assert!(!is_same_site_path("/a\r\nSet-Cookie: x"));
    }

    #[test]
    fn reads_cookies_by_name() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("a=1; pc_session=x"),
        );
        headers.append(header::COOKIE, HeaderValue::from_static("pc_session=y"));
        assert_eq!(
            cookies(&headers, SESSION_COOKIE).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }
}
