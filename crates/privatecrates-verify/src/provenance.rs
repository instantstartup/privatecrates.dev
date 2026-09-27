//! Checks a version's provenance: the GitHub Actions OIDC token stored with its release (SPEC §10.3).

use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use privatecrates_common::{audience, storage::Owner};
use serde::Deserialize;

pub struct Expected<'a> {
    pub issuer: &'a str,
    pub base_url: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub cksum: &'a str,
    /// The crate's owners when the version was appended.
    pub owner: Option<&'a Owner>,
}

#[derive(Deserialize)]
struct Claims {
    repository: String,
    repository_id: String,
    job_workflow_ref: String,
    #[serde(default)]
    environment: Option<String>,
}

pub fn check(jwt: &[u8], jwks: &JwkSet, expected: &Expected<'_>) -> Result<(), String> {
    let jwt = std::str::from_utf8(jwt)
        .map_err(|_| "not text".to_owned())?
        .trim();
    let header = jsonwebtoken::decode_header(jwt).map_err(|e| e.to_string())?;
    let kid = header.kid.ok_or("no key ID")?;
    let jwk = jwks.find(&kid).ok_or(
        "it was signed by a key GitHub no longer publishes, so it can no longer be checked; run the verifier more \
         often",
    )?;
    let key = DecodingKey::from_jwk(jwk).map_err(|e| e.to_string())?;
    let audience = audience::publish(
        expected.base_url,
        expected.name,
        expected.version,
        expected.cksum,
    );
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[expected.issuer]);
    validation.set_audience(&[&audience]);
    // Provenance is checked long after the token expired; the signature and audience are what matter.
    validation.validate_exp = false;
    validation.set_required_spec_claims(&["iss", "aud"]);
    let claims = jsonwebtoken::decode::<Claims>(jwt, &key, &validation)
        .map_err(|e| format!("{e} (expected audience {audience})"))?
        .claims;
    let owner = expected
        .owner
        .ok_or("the crate had no owners file when it was published")?;
    if claims.repository_id != owner.repository_id.to_string() {
        return Err(format!(
            "it was published from {} (ID {}), not the owning repository {}",
            claims.repository, claims.repository_id, owner.repository
        ));
    }
    let workflow = claims
        .job_workflow_ref
        .split_once('@')
        .and_then(|(path, _)| path.strip_prefix(&claims.repository))
        .and_then(|rest| rest.strip_prefix("/.github/workflows/"))
        .ok_or_else(|| {
            format!(
                "its workflow {} is not in its own repository",
                claims.job_workflow_ref
            )
        })?;
    if !owner.publish_workflows.iter().any(|w| w == workflow) {
        return Err(format!(
            "its workflow {workflow} was not allowed to publish"
        ));
    }
    if let Some(env) = &owner.publish_environment
        && claims.environment.as_ref() != Some(env)
    {
        return Err(format!("it was not published from the `{env}` environment"));
    }
    Ok(())
}
