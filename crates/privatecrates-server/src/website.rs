//! The website: the static SvelteKit build, served on the apex host with its security and cache headers.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_status::SetStatus,
};

use crate::{AppState, error::ApiError};

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Pages change with every deployment, so browsers revalidate them.
const REVALIDATE: &str = "no-cache";
const SHORT: &str = "public, max-age=3600";

pub struct Website {
    files: Option<Files>,
    /// Allows only our own scripts, plus the prerendered pages' inline bootstrap scripts by hash.
    csp: HeaderValue,
}

struct Files {
    dir: PathBuf,
    serve: ServeDir<SetStatus<ServeFile>>,
}

impl Website {
    pub fn new(dir: Option<&Path>) -> Self {
        let mut hashes = BTreeSet::new();
        if let Some(dir) = dir {
            if !dir.join("index.html").is_file() {
                log::warn!(dir:% = dir.display(); "the website directory has no index.html");
            }
            collect_inline_script_hashes(dir, &mut hashes);
        }
        Self {
            files: dir.map(|dir| Files {
                dir: dir.to_owned(),
                // The build writes Brotli and gzip copies beside each file (website/scripts/precompress.js):
                // served as they are, they cost no CPU per request.
                serve: ServeDir::new(dir)
                    .precompressed_br()
                    .precompressed_gzip()
                    .not_found_service(ServeFile::new(dir.join("404.html"))),
            }),
            csp: content_security_policy(&hashes),
        }
    }
}

fn content_security_policy(script_hashes: &BTreeSet<String>) -> HeaderValue {
    let scripts: String = script_hashes
        .iter()
        .map(|h| format!(" 'sha256-{h}'"))
        .collect();
    HeaderValue::from_str(&format!(
        "default-src 'self'; script-src 'self'{scripts}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: https://avatars.githubusercontent.com; font-src 'self'; connect-src 'self'; \
         object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    ))
    .expect("the policy is ASCII")
}

/// SvelteKit's prerendered pages start the app with an inline script; the policy allows exactly those scripts.
fn collect_inline_script_hashes(dir: &Path, hashes: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        log::warn!(dir:% = dir.display(); "cannot read the website directory");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_inline_script_hashes(&path, hashes);
        } else if path.extension().is_some_and(|e| e == "html")
            && let Ok(html) = std::fs::read_to_string(&path)
        {
            hashes.extend(inline_scripts(&html).into_iter().map(|script| {
                base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script))
            }));
        }
    }
}

fn inline_scripts(html: &str) -> Vec<&str> {
    let mut scripts = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let tag_and_body = &rest[start..];
        let Some(tag_end) = tag_and_body.find('>') else {
            break;
        };
        let body = &tag_and_body[tag_end + 1..];
        let Some(close) = body.find("</script>") else {
            break;
        };
        if !tag_and_body[..tag_end].contains(" src=") {
            scripts.push(&body[..close]);
        }
        rest = &body[close..];
    }
    scripts
}

/// Prerendered pages are `route.html` or `route/index.html`. `ServeDir` finds the second, but for `/docs` it would
/// see the `docs/` directory and miss `docs.html`, so try that first. Only plain paths are considered.
async fn prefer_html_file(dir: &Path, request: &mut Request) {
    let path = request.uri().path();
    let plain = path.split('/').skip(1).all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    if path == "/" || !plain {
        return;
    }
    let file = dir.join(format!("{}.html", &path[1..]));
    if tokio::fs::metadata(&file).await.is_ok_and(|m| m.is_file()) {
        let rewritten = match request.uri().query() {
            Some(query) => format!("{path}.html?{query}"),
            None => format!("{path}.html"),
        };
        if let Ok(uri) = rewritten.parse() {
            *request.uri_mut() = uri;
        }
    }
}

/// Serves the website's files; anything missing gets `404.html` with a 404 status.
pub async fn serve(State(state): State<Arc<AppState>>, mut request: Request) -> Response {
    let Some(files) = &state.website.files else {
        return ApiError::NotFound.into_response();
    };
    prefer_html_file(&files.dir, &mut request).await;
    let immutable = request.uri().path().starts_with("/_app/immutable/");
    let Ok(mut response) = files.serve.clone().oneshot(request).await;
    // The build is UTF-8, but `ServeDir` names no charset, so a browser could guess another for a text file such as
    // `/.well-known/security.txt`.
    if let Some(text) = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.starts_with("text/") && !v.contains("charset"))
        .and_then(|v| HeaderValue::from_str(&format!("{v}; charset=utf-8")).ok())
    {
        response.headers_mut().insert(header::CONTENT_TYPE, text);
    }
    let html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    let cache = if immutable && response.status().is_success() {
        IMMUTABLE
    } else if html || !response.status().is_success() {
        REVALIDATE
    } else {
        SHORT
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response.into_response()
}

/// `robots.txt` outside production: keeps the whole deployment out of search engines.
pub async fn disallow_robots() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, SHORT),
        ],
        "User-agent: *\nDisallow: /\n",
    )
        .into_response()
}

/// The website's security headers on every apex response, and outside production a request not to index it. HSTS
/// and `nosniff` are set for every host by [`crate::transport_headers`].
pub async fn security_headers(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, state.website.csp.clone());
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // Nothing on the site needs these; sign-in and Checkout are redirects, never pop-ups.
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    if !state.config.is_production() {
        headers.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_inline_scripts_only() {
        let html = r#"<head><script type="module" src="/_app/x.js"></script></head>
<body><script>
  start();
</script><script type="module">go()</script></body>"#;
        assert_eq!(inline_scripts(html), ["\n  start();\n", "go()"]);
    }

    #[test]
    fn the_policy_lists_script_hashes() {
        let csp = content_security_policy(&BTreeSet::from(["abc=".to_owned()]));
        let csp = csp.to_str().unwrap();
        assert!(csp.contains("script-src 'self' 'sha256-abc=';"), "{csp}");
        assert!(csp.contains("https://avatars.githubusercontent.com"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }
}
