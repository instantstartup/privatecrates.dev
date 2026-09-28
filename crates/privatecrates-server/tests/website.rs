//! Routing by host, and the static website on the apex host.

mod common;

use std::fs;

use base64::Engine;
use common::{Harness, Options};
use sha2::{Digest, Sha256};

const BOOT: &str = "\n  __sveltekit_boot.start();\n";

fn website() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, content: &str| {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    };
    write(
        "index.html",
        &format!(
            "<!doctype html><title>Home</title><script type=\"module\" src=\"/_app/immutable/start.js\"></script><script>{BOOT}</script>"
        ),
    );
    write("docs.html", "<!doctype html><title>Docs</title>");
    write("docs/setup.html", "<!doctype html><title>Setup</title>");
    write(
        "pricing/index.html",
        "<!doctype html><title>Pricing</title>",
    );
    write("404.html", "<!doctype html><title>Not found</title>");
    write("_app/immutable/start.js", "export {}");
    write("robots.txt", "User-agent: *");
    write(
        ".well-known/security.txt",
        "Contact: mailto:security@privatecrates.dev\n",
    );
    dir
}

async fn start() -> (Harness, tempfile::TempDir) {
    let dir = website();
    let h = Harness::start_with(Options {
        website_dir: Some(dir.path().to_owned()),
        ..Options::default()
    })
    .await;
    (h, dir)
}

async fn page(
    h: &Harness,
    path: &str,
) -> (reqwest::StatusCode, reqwest::header::HeaderMap, String) {
    let response = h.client.get(h.apex(path)).send().await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    (status, headers, response.text().await.unwrap())
}

#[tokio::test]
async fn serves_the_prerendered_pages() {
    let (h, _dir) = start().await;
    let (status, headers, body) = page(&h, "/").await;
    assert_eq!(status, 200);
    assert!(body.contains("<title>Home</title>"));
    assert_eq!(headers["cache-control"], "no-cache");
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );

    for (path, title) in [
        ("/docs", "Docs"),
        ("/docs/setup", "Setup"),
        ("/pricing/", "Pricing"),
        ("/docs?section=ci", "Docs"),
    ] {
        let (status, _, body) = page(&h, path).await;
        assert_eq!(status, 200, "{path}");
        assert!(
            body.contains(&format!("<title>{title}</title>")),
            "{path}: {body}"
        );
    }

    let (status, headers, _) = page(&h, "/_app/immutable/start.js").await;
    assert_eq!(status, 200);
    assert_eq!(
        headers["cache-control"],
        "public, max-age=31536000, immutable"
    );
    // Not production: the website's own robots.txt is replaced, and nothing may be indexed.
    let (status, headers, body) = page(&h, "/robots.txt").await;
    assert_eq!(status, 200);
    assert_eq!(body, "User-agent: *\nDisallow: /\n");
    assert_eq!(headers["cache-control"], "public, max-age=3600");
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );

    for path in [
        "/missing",
        "/docs/missing",
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
    ] {
        let (status, headers, body) = page(&h, path).await;
        assert_eq!(status, 404, "{path}");
        assert!(body.contains("<title>Not found</title>"), "{path}: {body}");
        assert_eq!(headers["cache-control"], "no-cache");
    }
}

#[tokio::test]
async fn serves_security_txt_from_the_build() {
    let (h, _dir) = start().await;
    let (status, headers, body) = page(&h, "/.well-known/security.txt").await;
    assert_eq!(status, 200);
    assert_eq!(body, "Contact: mailto:security@privatecrates.dev\n");
    assert_eq!(headers["content-type"], "text/plain; charset=utf-8");
    assert_eq!(headers["cache-control"], "public, max-age=3600");
}

#[tokio::test]
async fn apex_responses_carry_security_headers() {
    let (h, _dir) = start().await;
    for path in ["/", "/missing", "/api/session"] {
        let (_, headers, _) = page(&h, path).await;
        let csp = headers["content-security-policy"].to_str().unwrap();
        let boot = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(BOOT));
        assert!(
            csp.contains(&format!("script-src 'self' 'sha256-{boot}';")),
            "{csp}"
        );
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("img-src 'self' data: https://avatars.githubusercontent.com"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert_eq!(
            headers["referrer-policy"],
            "strict-origin-when-cross-origin"
        );
        assert_eq!(headers["x-frame-options"], "DENY");
        // The harness serves plain http, so no HSTS.
        assert!(headers.get("strict-transport-security").is_none());
        // Only production may be indexed.
        assert_eq!(headers["x-robots-tag"], "noindex");
    }
}

#[tokio::test]
async fn routes_by_host() {
    let (h, _dir) = start().await;
    let get = |url: String| h.client.get(url).send();

    // The registry is on the tenant host only.
    assert_eq!(
        get(h.url("/index/config.json")).await.unwrap().status(),
        401
    );
    let (status, _, body) = page(&h, "/index/config.json").await;
    assert_eq!(status, 404);
    assert!(body.contains("Not found"));
    // The account API is on the apex only.
    assert_eq!(get(h.url("/api/session")).await.unwrap().status(), 404);
    assert_eq!(get(h.apex("/api/session")).await.unwrap().status(), 200);

    let www = get(format!("http://www.localhost:{}/docs?x=1", h.port))
        .await
        .unwrap();
    assert_eq!(www.status(), 301);
    assert_eq!(www.headers()["location"], h.apex("/docs?x=1"));

    // Reserved names are never tenants and other hosts get nothing; a name with no registry answers like one
    // (SPEC §6.7).
    for url in [
        format!("http://api.localhost:{}/index/config.json", h.port),
        format!("http://api.localhost:{}/", h.port),
    ] {
        assert_eq!(get(url.clone()).await.unwrap().status(), 404, "{url}");
    }
    let unknown = format!("http://other.localhost:{}/index/config.json", h.port);
    assert_eq!(get(unknown).await.unwrap().status(), 401);
    let foreign = h
        .client
        .get(h.apex("/"))
        .header("Host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), 404);

    for url in [
        h.apex("/healthz"),
        h.url("/healthz"),
        format!("http://other.localhost:{}/healthz", h.port),
    ] {
        let response = get(url.clone()).await.unwrap();
        assert_eq!(response.status(), 200, "{url}");
        assert_eq!(response.text().await.unwrap(), "ok");
    }

    // Every host's responses carry the transport headers, not just the website's.
    for url in [
        h.url("/index/config.json"),
        h.url("/healthz"),
        format!("http://www.localhost:{}/", h.port),
        format!("http://other.localhost:{}/", h.port),
    ] {
        let response = get(url.clone()).await.unwrap();
        assert_eq!(
            response.headers()["x-content-type-options"],
            "nosniff",
            "{url}"
        );
    }
}

#[tokio::test]
async fn without_a_website_the_apex_serves_only_the_api() {
    let h = Harness::start().await;
    let response = h.client.get(h.apex("/")).send().await.unwrap();
    assert_eq!(response.status(), 404);
    let response = h.client.get(h.apex("/api/session")).send().await.unwrap();
    assert_eq!(response.status(), 200);
}

/// The website's workflows pin the same release as the server's (privatecrates_common::install).
#[test]
fn the_website_pins_this_release() {
    let snippets = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../website/src/lib/snippets.ts"),
    )
    .unwrap();
    let version = privatecrates_common::install::ci_step("x")
        .lines()
        .find_map(|line| line.split("VERSION: \"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    assert!(
        snippets.contains(&format!("export const RELEASE_VERSION = '{version}';")),
        "website/src/lib/snippets.ts must pin RELEASE_VERSION = '{version}'"
    );
}
