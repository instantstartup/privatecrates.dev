//! `cargo privatecrates init`: configures a crate repository or workspace to use and publish to a registry.
//!
//! Every edit is a merge that keeps the file's formatting and comments, and running it again changes nothing. A file
//! it did not write is replaced only with `--force`.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::Serialize;
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::{
    error::Error,
    project::{self, Project},
    target,
};

/// The credential provider, as `.cargo/config.toml` names it.
pub const PROVIDER: &str = "cargo-credential-privatecrates";

pub struct Options<'a> {
    pub slug: &'a str,
    /// The registry's base URL, e.g. `https://acme.privatecrates.dev`.
    pub url: &'a str,
    pub workflow_name: &'a str,
    pub force: bool,
}

#[derive(Serialize)]
pub struct Report {
    pub root: PathBuf,
    pub registry: String,
    pub registry_url: String,
    pub workspace: bool,
    /// The repository `package.repository` is set from, when `origin` is on GitHub.
    pub repository: Option<String>,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct Change {
    pub path: PathBuf,
    pub action: Action,
    pub detail: String,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Created,
    Updated,
    Unchanged,
    /// An existing file that already does the job, left as it is.
    Kept,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
            Self::Kept => "kept",
        })
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Configured {} for the registry {} ({}):",
            self.root.display(),
            self.registry,
            self.registry_url
        )?;
        for change in &self.changes {
            let path = change.path.strip_prefix(&self.root).unwrap_or(&change.path);
            write!(f, "  {:<9} {}", change.action.to_string(), path.display())?;
            if !change.detail.is_empty() {
                write!(f, ": {}", change.detail)?;
            }
            writeln!(f)?;
        }
        for warning in &self.warnings {
            writeln!(f, "warning: {warning}")?;
        }
        if self.changes.iter().all(|c| c.action == Action::Unchanged) {
            write!(f, "Nothing to change.")
        } else {
            write!(
                f,
                "Next: commit this and open a pull request. Once it is merged, push a tag such as v0.1.0 to publish \
                 from GitHub Actions."
            )
        }
    }
}

pub fn run(dir: &Path, options: &Options<'_>) -> Result<Report, Error> {
    if !privatecrates_common::slug_is_valid(options.slug) {
        return Err(Error::Invalid(format!(
            "`{}` is not a registry name: 1 to 63 lowercase letters, digits or hyphens",
            options.slug
        )));
    }
    let name = options.workflow_name;
    if !(name.ends_with(".yml") || name.ends_with(".yaml")) || name.contains(['/', '\\']) {
        return Err(Error::Invalid(format!(
            "`{name}` is not a workflow file name such as publish.yml"
        )));
    }
    let mut warnings = Vec::new();
    let index = target::index_url(options.url);
    let configure = |root: &Path| {
        let config = root.join(".cargo/config.toml");
        edit_toml(&config, |doc| {
            add_registry(doc, options.slug, &index, options.force)
                .map_err(|detail| Error::Conflict {
                    path: config.clone(),
                    detail,
                })
                .map(|changed| changed.then(|| format!("[registries.{}] → {index}", options.slug)))
        })
    };
    let (project, configured) = match Project::load(dir) {
        Ok(project) => {
            let configured = configure(&project.root)?;
            (project, configured)
        }
        // Cargo cannot read manifests that depend on the registry's crates until the registry is configured.
        Err(e) if dir.join("Cargo.toml").is_file() => {
            let configured = configure(dir)?;
            (Project::load(dir).map_err(|_| e)?, configured)
        }
        Err(e) => return Err(e),
    };
    let mut changes = vec![configured];
    let repository = project::github_remote(&project.root);
    match &repository {
        Some(repository) => changes.extend(set_repositories(&project, repository)?),
        None => warnings.push(
            "the `origin` remote is not a GitHub repository, so `package.repository` was not set; set it to the \
             crate's repository in your organisation"
                .into(),
        ),
    }

    let git_root = project::git_root(&project.root).unwrap_or_else(|| project.root.clone());
    let working_directory = project
        .root
        .strip_prefix(&git_root)
        .ok()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let path = git_root.join(".github/workflows").join(name);
    let content = workflow(
        options.slug,
        project.workspace,
        working_directory.as_deref(),
    );
    changes.push(write_workflow(
        &path,
        &content,
        options.slug,
        options.force,
    )?);

    Ok(Report {
        root: project.root,
        registry: options.slug.to_owned(),
        registry_url: options.url.to_owned(),
        workspace: project.workspace,
        repository,
        changes,
        warnings,
    })
}

/// Applies `edit` to a TOML file (or a new one), writing it only if the edit changed something. `edit` returns a
/// description of the change, or `None` when there was nothing to do.
fn edit_toml(
    path: &Path,
    edit: impl FnOnce(&mut DocumentMut) -> Result<Option<String>, Error>,
) -> Result<Change, Error> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Error::io("read", path)(e)),
    };
    let mut doc = existing
        .as_deref()
        .unwrap_or_default()
        .parse::<DocumentMut>()
        .map_err(|source| Error::Toml {
            path: path.to_owned(),
            source,
        })?;
    let Some(detail) = edit(&mut doc)? else {
        return Ok(Change {
            path: path.to_owned(),
            action: Action::Unchanged,
            detail: String::new(),
        });
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(Error::io("create", dir))?;
    }
    std::fs::write(path, doc.to_string()).map_err(Error::io("write", path))?;
    Ok(Change {
        path: path.to_owned(),
        action: if existing.is_some() {
            Action::Updated
        } else {
            Action::Created
        },
        detail,
    })
}

/// Adds `[registries.<slug>]` with the index and the credential provider. `Ok(false)` when it is already there; an
/// error describing the difference when the registry is configured otherwise, unless `force`.
pub fn add_registry(
    doc: &mut DocumentMut,
    slug: &str,
    index: &str,
    force: bool,
) -> Result<bool, String> {
    let registries = doc
        .entry("registries")
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        })
        .as_table_like_mut()
        .ok_or("`registries` is not a table")?;
    let entry = registries
        .entry(slug)
        .or_insert(Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| format!("`registries.{slug}` is not a table"))?;
    let current_index = entry.get("index").and_then(Item::as_str);
    let current_provider = entry
        .get("credential-provider")
        .map(|p| is_privatecrates_provider(p.as_value()));
    let mut changed = false;
    match current_index {
        Some(current) if current == index => {}
        Some(current) if !force => {
            return Err(format!(
                "`registries.{slug}.index` is {current}, not {index}"
            ));
        }
        _ => {
            entry.insert("index", value(index));
            changed = true;
        }
    }
    match current_provider {
        Some(true) => {}
        Some(false) if !force => {
            return Err(format!(
                "`registries.{slug}.credential-provider` is not {PROVIDER}"
            ));
        }
        _ => {
            entry.insert("credential-provider", value(Array::from_iter([PROVIDER])));
            changed = true;
        }
    }
    Ok(changed)
}

/// Whether a `credential-provider` value runs this registry's provider, by name or by path.
pub fn is_privatecrates_provider(value: Option<&toml_edit::Value>) -> bool {
    let command = match value {
        Some(toml_edit::Value::Array(array)) => array.get(0).and_then(|v| v.as_str()),
        Some(toml_edit::Value::String(s)) => s.value().split_whitespace().next(),
        _ => None,
    };
    command.is_some_and(|c| {
        let file = c.rsplit(['/', '\\']).next().unwrap_or(c);
        file.strip_suffix(".exe").unwrap_or(file) == PROVIDER
    })
}

/// Sets `package.repository` where it is missing: in `[workspace.package]` with `repository.workspace = true` in
/// the members of a workspace, or directly in a single crate.
fn set_repositories(project: &Project, repository: &str) -> Result<Vec<Change>, Error> {
    let mut changes = Vec::new();
    let root_manifest = project.root.join("Cargo.toml");
    if project.workspace {
        changes.push(edit_toml(&root_manifest, |doc| {
            Ok(set_workspace_repository(doc, repository)
                .then(|| format!("workspace.package.repository = \"{repository}\"")))
        })?);
    }
    for package in &project.packages {
        let change = edit_toml(&package.manifest_path, |doc| {
            Ok(if project.workspace {
                inherit_repository(doc)
                    .then(|| format!("{}: repository.workspace = true", package.name))
            } else {
                set_package_repository(doc, repository)
                    .then(|| format!("{}: repository = \"{repository}\"", package.name))
            })
        })?;
        // The root package of a workspace shares the root manifest: report the file once.
        match changes.iter_mut().find(|c| c.path == change.path) {
            Some(existing) if change.action != Action::Unchanged => {
                existing.action = Action::Updated;
                existing.detail = [existing.detail.as_str(), &change.detail]
                    .into_iter()
                    .filter(|d| !d.is_empty())
                    .collect::<Vec<_>>()
                    .join("; ");
            }
            Some(_) => {}
            None => changes.push(change),
        }
    }
    Ok(changes)
}

/// The `[package]` table, if there is one.
fn package(doc: &mut DocumentMut) -> Option<&mut dyn toml_edit::TableLike> {
    doc.get_mut("package")?.as_table_like_mut()
}

pub fn set_package_repository(doc: &mut DocumentMut, repository: &str) -> bool {
    match package(doc) {
        Some(package) if !package.contains_key("repository") => {
            package.insert("repository", value(repository));
            true
        }
        _ => false,
    }
}

pub fn set_workspace_repository(doc: &mut DocumentMut, repository: &str) -> bool {
    let Some(workspace) = doc.get_mut("workspace").and_then(Item::as_table_like_mut) else {
        return false;
    };
    let package = workspace
        .entry("package")
        .or_insert(Item::Table(Table::new()))
        .as_table_like_mut();
    match package {
        Some(package) if !package.contains_key("repository") => {
            package.insert("repository", value(repository));
            true
        }
        _ => false,
    }
}

/// Adds `repository.workspace = true` to a member without a repository.
pub fn inherit_repository(doc: &mut DocumentMut) -> bool {
    match package(doc) {
        Some(package) if !package.contains_key("repository") => {
            let mut inherit = Table::new();
            inherit.set_dotted(true);
            inherit.insert("workspace", value(true));
            package.insert("repository", Item::Table(inherit));
            true
        }
        _ => false,
    }
}

/// The publishing workflow, as on the website (`ciPublish` in website/src/lib/snippets.ts).
pub fn workflow(slug: &str, workspace: bool, working_directory: Option<&str>) -> String {
    let publish = if workspace {
        format!("cargo publish --workspace --registry {slug}")
    } else {
        format!("cargo publish --registry {slug}")
    };
    let defaults = working_directory
        .map(|dir| format!("    defaults:\n      run:\n        working-directory: {dir}\n"))
        .unwrap_or_default();
    format!(
        "# Publishes to the PrivateCrates registry `{slug}` when a tag such as v1.2.3 is pushed. The job's OIDC token
# is the credential and the provenance: no secrets. Written by `cargo privatecrates init`.
name: publish
on:
  push:
    tags: [\"v*\"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
{defaults}    steps:
      - uses: actions/checkout@v5
      - uses: cargo-bins/cargo-binstall@main   # pin to a commit
      - run: cargo binstall --no-confirm {PROVIDER}
      - run: {publish}
"
    )
}

/// Writes the workflow, unless an existing one already publishes to the registry or `force` is not given.
fn write_workflow(path: &Path, content: &str, slug: &str, force: bool) -> Result<Change, Error> {
    let change = |action, detail: &str| Change {
        path: path.to_owned(),
        action,
        detail: detail.to_owned(),
    };
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Error::io("read", path)(e)),
    };
    let action = match existing.as_deref() {
        Some(text) if text == content => return Ok(change(Action::Unchanged, "")),
        Some(text) if !force && publishes_to(text, slug) => {
            return Ok(change(
                Action::Kept,
                "already publishes to the registry; --force replaces it",
            ));
        }
        Some(_) if !force => {
            return Err(Error::Conflict {
                path: path.to_owned(),
                detail: format!("it does not publish to the registry {slug}"),
            });
        }
        Some(_) => Action::Updated,
        None => Action::Created,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(Error::io("create", dir))?;
    }
    std::fs::write(path, content).map_err(Error::io("write", path))?;
    Ok(change(action, "publishes on tags v*"))
}

/// Whether a workflow runs `cargo publish` for the registry.
pub fn publishes_to(workflow: &str, slug: &str) -> bool {
    workflow.lines().any(|line| {
        let words: Vec<&str> = line.split_whitespace().collect();
        words.windows(2).any(|w| w == ["cargo", "publish"])
            && (words.windows(2).any(|w| w == ["--registry", slug])
                || words.contains(&format!("--registry={slug}").as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> DocumentMut {
        text.parse().unwrap()
    }

    const INDEX: &str = "sparse+https://acme.privatecrates.dev/index/";

    #[test]
    fn adds_a_registry_keeping_comments_and_formatting() {
        let mut config = doc("# Our Cargo settings.\n\
             [build]\n\
             rustflags = [\"-D\", \"warnings\"]   # keep it clean\n\
             \n\
             [registries.other]\n\
             index = \"sparse+https://other.example/index/\"\n");
        assert_eq!(add_registry(&mut config, "acme", INDEX, false), Ok(true));
        assert_eq!(
            config.to_string(),
            "# Our Cargo settings.\n\
             [build]\n\
             rustflags = [\"-D\", \"warnings\"]   # keep it clean\n\
             \n\
             [registries.other]\n\
             index = \"sparse+https://other.example/index/\"\n\
             \n\
             [registries.acme]\n\
             index = \"sparse+https://acme.privatecrates.dev/index/\"\n\
             credential-provider = [\"cargo-credential-privatecrates\"]\n"
        );
        // Again: nothing to do.
        let before = config.to_string();
        assert_eq!(add_registry(&mut config, "acme", INDEX, false), Ok(false));
        assert_eq!(config.to_string(), before);
    }

    #[test]
    fn adds_a_registry_to_an_empty_config() {
        let mut config = doc("");
        assert_eq!(add_registry(&mut config, "acme", INDEX, false), Ok(true));
        assert_eq!(
            config.to_string(),
            "[registries.acme]\n\
             index = \"sparse+https://acme.privatecrates.dev/index/\"\n\
             credential-provider = [\"cargo-credential-privatecrates\"]\n"
        );
    }

    #[test]
    fn a_differently_configured_registry_needs_force() {
        let text =
            "[registries.acme]\nindex = \"sparse+https://elsewhere.example/index/\" # theirs\n";
        let mut config = doc(text);
        let refused = add_registry(&mut config, "acme", INDEX, false).unwrap_err();
        assert!(refused.contains("elsewhere.example"), "{refused}");
        assert_eq!(config.to_string(), text);
        assert_eq!(add_registry(&mut config, "acme", INDEX, true), Ok(true));
        assert!(config.to_string().contains(INDEX));

        let text = "[registries.acme]\nindex = \"sparse+https://acme.privatecrates.dev/index/\"\n\
                    credential-provider = \"cargo:token\"\n";
        let mut config = doc(text);
        assert!(add_registry(&mut config, "acme", INDEX, false).is_err());
        assert_eq!(add_registry(&mut config, "acme", INDEX, true), Ok(true));
        assert!(
            config
                .to_string()
                .contains("credential-provider = [\"cargo-credential-privatecrates\"]")
        );
    }

    #[test]
    fn a_provider_given_by_path_is_accepted() {
        let mut config = doc(
            "[registries.acme]\nindex = \"sparse+https://acme.privatecrates.dev/index/\"\n\
             credential-provider = [\"/opt/bin/cargo-credential-privatecrates\"]\n",
        );
        assert_eq!(add_registry(&mut config, "acme", INDEX, false), Ok(false));
        // Only the index was there: the provider is added.
        let mut config =
            doc("[registries.acme]\nindex = \"sparse+https://acme.privatecrates.dev/index/\"\n");
        assert_eq!(add_registry(&mut config, "acme", INDEX, false), Ok(true));
        assert!(config.to_string().contains("credential-provider"));
    }

    #[test]
    fn sets_a_single_crates_repository() {
        let text = "[package]\nname = \"story_engine\" # the engine\nversion = \"0.1.0\"\n\n[dependencies]\n";
        let mut manifest = doc(text);
        assert!(set_package_repository(
            &mut manifest,
            "https://github.com/acme/story-engine"
        ));
        assert_eq!(
            manifest.to_string(),
            "[package]\nname = \"story_engine\" # the engine\nversion = \"0.1.0\"\n\
             repository = \"https://github.com/acme/story-engine\"\n\n[dependencies]\n"
        );
        // An existing repository is left alone.
        assert!(!set_package_repository(
            &mut manifest,
            "https://github.com/acme/other"
        ));
        assert!(manifest.to_string().contains("acme/story-engine"));
    }

    #[test]
    fn a_workspace_sets_the_repository_once_and_members_inherit_it() {
        let mut root =
            doc("[workspace]\nmembers = [\"crates/*\"] # all of them\nresolver = \"3\"\n");
        assert!(set_workspace_repository(
            &mut root,
            "https://github.com/acme/story"
        ));
        assert_eq!(
            root.to_string(),
            "[workspace]\nmembers = [\"crates/*\"] # all of them\nresolver = \"3\"\n\n\
             [workspace.package]\nrepository = \"https://github.com/acme/story\"\n"
        );
        assert!(!set_workspace_repository(
            &mut root,
            "https://github.com/acme/story"
        ));

        let mut root =
            doc("[workspace]\nmembers = []\n\n[workspace.package]\nedition = \"2024\"\n");
        assert!(set_workspace_repository(
            &mut root,
            "https://github.com/acme/story"
        ));
        assert!(
            root.to_string()
                .ends_with("[workspace.package]\nedition = \"2024\"\nrepository = \"https://github.com/acme/story\"\n")
        );

        let mut member = doc("[package]\nname = \"story_core\"\nversion = \"0.1.0\"\n");
        assert!(inherit_repository(&mut member));
        assert_eq!(
            member.to_string(),
            "[package]\nname = \"story_core\"\nversion = \"0.1.0\"\nrepository.workspace = true\n"
        );
        assert!(!inherit_repository(&mut member));
        let mut own = doc("[package]\nname = \"x\"\nrepository = \"https://github.com/acme/x\"\n");
        assert!(!inherit_repository(&mut own));
    }

    #[test]
    fn replaces_a_workflow_only_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".github/workflows/publish.yml");
        let content = workflow("acme", false, None);
        let write = |force| write_workflow(&path, &content, "acme", force).map(|c| c.action);
        assert_eq!(write(false).unwrap(), Action::Created);
        assert_eq!(write(false).unwrap(), Action::Unchanged);

        // Edited, e.g. to pin the action: still publishes to the registry, so it is kept.
        let pinned = content.replace("@main", "@0123abcd");
        std::fs::write(&path, &pinned).unwrap();
        assert_eq!(write(false).unwrap(), Action::Kept);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), pinned);

        // Someone else's workflow of the same name.
        std::fs::write(&path, "name: docs\n").unwrap();
        assert!(matches!(write(false), Err(Error::Conflict { .. })));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "name: docs\n");
        assert_eq!(write(true).unwrap(), Action::Updated);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn the_workflow_matches_the_websites() {
        assert_eq!(
            workflow("acme", false, None),
            "# Publishes to the PrivateCrates registry `acme` when a tag such as v1.2.3 is pushed. The job's OIDC token
# is the credential and the provenance: no secrets. Written by `cargo privatecrates init`.
name: publish
on:
  push:
    tags: [\"v*\"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: cargo-bins/cargo-binstall@main   # pin to a commit
      - run: cargo binstall --no-confirm cargo-credential-privatecrates
      - run: cargo publish --registry acme
"
        );
        let workspace = workflow("acme", true, Some("rust"));
        assert!(workspace.contains("      - run: cargo publish --workspace --registry acme\n"));
        assert!(workspace.contains(
            "    runs-on: ubuntu-latest\n    defaults:\n      run:\n        working-directory: rust\n    steps:\n"
        ));
        assert!(publishes_to(&workspace, "acme"));
        assert!(!publishes_to(&workspace, "acme2"));
        assert!(publishes_to(
            "run: cargo publish -p x --registry=acme",
            "acme"
        ));
        assert!(!publishes_to("run: cargo build --registry acme", "acme"));
    }
}
