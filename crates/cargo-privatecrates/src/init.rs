//! `cargo privatecrates init`: configures a crate repository or workspace to use and publish to a registry.
//!
//! Every edit is a merge that keeps the file's formatting and comments, and running it again changes nothing. A file
//! it did not write is replaced only with `--force`. [`run`] only plans: the edits are held in [`Staged`] until the
//! person (or `--yes`) agrees, and [`Staged::apply`] writes them.

use std::{
    collections::BTreeMap,
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
    /// Also write the GitHub Actions workflow that publishes.
    pub workflow: bool,
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
    /// Whether the changes were written, or are only planned.
    pub applied: bool,
}

impl Report {
    /// Whether applying would write anything.
    pub fn has_changes(&self) -> bool {
        self.changes
            .iter()
            .any(|c| matches!(c.action, Action::Created | Action::Updated))
    }

    /// The publish workflow, when this plan creates or replaces it.
    pub fn workflow_change(&self) -> Option<&Change> {
        self.changes
            .iter()
            .find(|c| c.workflow && matches!(c.action, Action::Created | Action::Updated))
    }

    /// Leaves the publish workflow out of the plan.
    pub fn drop_workflow(&mut self, staged: &mut Staged) {
        self.changes.retain(|c| {
            if c.workflow {
                staged.0.remove(&c.path);
            }
            !c.workflow
        });
    }
}

/// Files to write, held in memory until applied.
#[derive(Default)]
pub struct Staged(BTreeMap<PathBuf, String>);

impl Staged {
    /// The file as it will be: staged, or on disk. `None` when it does not exist.
    fn read(&self, path: &Path) -> Result<Option<String>, Error> {
        if let Some(text) = self.0.get(path) {
            return Ok(Some(text.clone()));
        }
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io("read", path)(e)),
        }
    }

    fn write(&mut self, path: &Path, content: String) {
        self.0.insert(path.to_owned(), content);
    }

    /// Writes every staged file.
    pub fn apply(&self) -> Result<(), Error> {
        for (path, content) in &self.0 {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(Error::io("create", dir))?;
            }
            std::fs::write(path, content).map_err(Error::io("write", path))?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Change {
    pub path: PathBuf,
    pub action: Action,
    pub detail: String,
    /// This is the publish workflow.
    #[serde(skip)]
    pub workflow: bool,
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

impl Action {
    /// What applying the plan will do.
    fn planned(self) -> &'static str {
        match self {
            Self::Created => "create",
            Self::Updated => "update",
            Self::Unchanged => "unchanged",
            Self::Kept => "keep",
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.applied {
            writeln!(
                f,
                "Configured {} for the registry {} ({}):",
                self.root.display(),
                self.registry,
                self.registry_url
            )?;
        } else {
            writeln!(
                f,
                "To configure {} for the registry {} ({}), init will:",
                self.root.display(),
                self.registry,
                self.registry_url
            )?;
        }
        for change in &self.changes {
            let path = change.path.strip_prefix(&self.root).unwrap_or(&change.path);
            let action = if self.applied {
                change.action.to_string()
            } else {
                change.action.planned().to_owned()
            };
            write!(f, "  {action:<9} {}", path.display())?;
            if !change.detail.is_empty() {
                write!(f, ": {}", change.detail)?;
            }
            writeln!(f)?;
        }
        for warning in &self.warnings {
            writeln!(f, "warning: {warning}")?;
        }
        if !self.has_changes() {
            write!(f, "Nothing to change.")
        } else if !self.applied {
            write!(f, "Nothing has been written yet.")
        } else if !self.changes.iter().any(|c| c.workflow) {
            write!(
                f,
                "Next: commit this and open a pull request. Crates are published from GitHub Actions: run init \
                 again without --no-workflow to add the workflow, or see {}/login#publish.",
                self.registry_url
            )
        } else {
            write!(
                f,
                "Next: commit this and open a pull request. Once it is merged, push a tag such as v0.1.0 to publish \
                 from GitHub Actions (in a workspace, story_engine-v0.1.0 publishes just that crate)."
            )
        }
    }
}

/// Plans the changes: nothing is written until the caller applies the [`Staged`] files.
pub fn run(dir: &Path, options: &Options<'_>) -> Result<(Report, Staged), Error> {
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
    let mut staged = Staged::default();
    let index = target::index_url(options.url);
    let configure = |staged: &mut Staged, root: &Path| {
        let config = root.join(".cargo/config.toml");
        edit_toml(staged, &config, |doc| {
            add_registry(doc, options.slug, &index, options.force)
                .map_err(|detail| Error::Conflict {
                    path: config.clone(),
                    detail,
                })
                .map(|changed| changed.then(|| format!("[registries.{}] → {index}", options.slug)))
        })
    };
    let (project, configured) = match Project::load(dir, None) {
        Ok(project) => {
            let configured = configure(&mut staged, &project.root)?;
            (project, configured)
        }
        // Cargo cannot read manifests that depend on the registry's crates until the registry is configured, and
        // nothing is written yet: tell Cargo about it on the command line.
        Err(e) if dir.join("Cargo.toml").is_file() => {
            let configured = configure(&mut staged, dir)?;
            let project = Project::load(dir, Some((options.slug, &index))).map_err(|_| e)?;
            (project, configured)
        }
        Err(e) => return Err(e),
    };
    let mut changes = vec![configured];
    let repository = project::github_remote(&project.root);
    if repository.is_none() {
        warnings.push(
            "the `origin` remote is not a GitHub repository, so `package.repository` was not set; set it to the \
             crate's repository in your organisation"
                .into(),
        );
    }
    changes.extend(edit_packages(
        &mut staged,
        &project,
        repository.as_deref(),
        options.slug,
        &mut warnings,
    )?);

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
    if options.workflow {
        changes.push(write_workflow(
            &mut staged,
            &path,
            &content,
            options.slug,
            options.force,
        )?);
    }

    Ok((
        Report {
            root: project.root,
            registry: options.slug.to_owned(),
            registry_url: options.url.to_owned(),
            workspace: project.workspace,
            repository,
            changes,
            warnings,
            applied: false,
        },
        staged,
    ))
}

/// Applies `edit` to a TOML file (or a new one), writing it only if the edit changed something. `edit` returns a
/// description of the change, or `None` when there was nothing to do.
fn edit_toml(
    staged: &mut Staged,
    path: &Path,
    edit: impl FnOnce(&mut DocumentMut) -> Result<Option<String>, Error>,
) -> Result<Change, Error> {
    let existing = staged.read(path)?;
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
            workflow: false,
        });
    };
    staged.write(path, doc.to_string());
    Ok(Change {
        path: path.to_owned(),
        action: if existing.is_some() {
            Action::Updated
        } else {
            Action::Created
        },
        detail,
        workflow: false,
    })
}

/// Adds `[registries.<slug>]` with the index and the credential provider. `Ok(false)` when it is already there; an
/// error describing the difference when the registry is configured otherwise, unless `force`. A new table gets a
/// comment for teammates who clone the repository: what to install, and the registry's page on joining.
pub fn add_registry(
    doc: &mut DocumentMut,
    slug: &str,
    index: &str,
    force: bool,
) -> Result<bool, String> {
    let blank_line = if doc.as_table().is_empty() { "" } else { "\n" };
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
        .or_insert_with(|| {
            let mut table = Table::new();
            table.decor_mut().set_prefix(format!(
                "{blank_line}# Needs cargo-credential-privatecrates (cargo binstall or cargo install it); the first \
                 build signs you in on GitHub, as does `cargo privatecrates login`.\n# Joining the team, editors and troubleshooting: {}\n",
                login_url(index)
            ));
            Item::Table(table)
        })
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

/// The registry's page for developers, from its index URL.
fn login_url(index: &str) -> String {
    let base = index.strip_prefix("sparse+").unwrap_or(index);
    let base = base.trim_end_matches('/');
    let base = base.strip_suffix("/index").unwrap_or(base);
    format!("{base}/login")
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

/// Edits each package's manifest:
/// - sets `package.repository` where it is missing (when the remote is known): in `[workspace.package]` with
///   `repository.workspace = true` in the members of a workspace, or directly in a single crate;
/// - restricts `publish` to the registry where it is unset, so a crate can never go to crates.io by accident.
fn edit_packages(
    staged: &mut Staged,
    project: &Project,
    repository: Option<&str>,
    slug: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<Change>, Error> {
    let mut changes = Vec::new();
    let root_manifest = project.root.join("Cargo.toml");
    let members: BTreeMap<String, String> = project
        .packages
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();
    // Crates whose `publish` leaves the registry out (`false` is an empty list); unset becomes the registry.
    let unpublished: Vec<&str> = project
        .packages
        .iter()
        .filter(|p| {
            p.publish
                .as_ref()
                .is_some_and(|r| !r.iter().any(|r| r == slug))
        })
        .map(|p| p.name.as_str())
        .collect();
    if project.workspace {
        changes.push(edit_toml(staged, &root_manifest, |doc| {
            let repository_change = repository.and_then(|repository| {
                set_workspace_repository(doc, repository)
                    .then(|| format!("workspace.package.repository = \"{repository}\""))
            });
            let details: Vec<String> = repository_change
                .into_iter()
                .chain(
                    pin_path_dependencies(doc, &members, slug)
                        .into_iter()
                        .map(|p| p.detail),
                )
                .collect();
            Ok((!details.is_empty()).then(|| details.join("; ")))
        })?);
    }
    for package in &project.packages {
        let change = edit_toml(staged, &package.manifest_path, |doc| {
            let repository_change = repository.and_then(|repository| {
                if project.workspace {
                    inherit_repository(doc)
                        .then(|| format!("{}: repository.workspace = true", package.name))
                } else {
                    set_package_repository(doc, repository)
                        .then(|| format!("{}: repository = \"{repository}\"", package.name))
                }
            });
            let publish_change = match restrict_publish(doc, slug) {
                Publish::Restricted => Some(format!("{}: publish = [\"{slug}\"]", package.name)),
                Publish::Elsewhere => {
                    warnings.push(format!(
                        "{}: its `publish` setting does not include \"{slug}\"; add it to publish this crate to the \
                         registry",
                        package.name
                    ));
                    None
                }
                Publish::Unpublished => {
                    warnings.push(format!(
                        "{}: `publish = false`, so it will not be published; if it should be, set \
                         publish = [\"{slug}\"] in {}",
                        package.name,
                        package.manifest_path.display()
                    ));
                    None
                }
                Publish::Unchanged => None,
            };
            let publishes = publishes_here(doc, slug);
            let details: Vec<String> = [repository_change, publish_change]
                .into_iter()
                .flatten()
                .chain(if publishes {
                    let pinned = pin_path_dependencies(doc, &members, slug);
                    for p in &pinned {
                        if unpublished.contains(&p.member.as_str()) {
                            warnings.push(format!(
                                "{} depends on {}, which is not published to {slug}: publish {} too, or \
                                 publishing {} fails",
                                package.name, p.member, p.member, package.name
                            ));
                        }
                    }
                    pinned.into_iter().map(|p| p.detail).collect()
                } else {
                    Vec::new()
                })
                .collect();
            Ok((!details.is_empty()).then(|| details.join("; ")))
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

/// What [`restrict_publish`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Publish {
    /// `publish = ["<slug>"]` was added.
    Restricted,
    /// Already allows the registry, or is inherited from the workspace: left alone.
    Unchanged,
    /// `publish = false`: left alone, and worth a warning, since it may be left from before the registry.
    Unpublished,
    /// Set, but without the registry: left alone, and worth a warning.
    Elsewhere,
}

/// Sets `publish = ["<slug>"]` where `publish` is unset.
pub fn restrict_publish(doc: &mut DocumentMut, slug: &str) -> Publish {
    let Some(package) = package(doc) else {
        return Publish::Unchanged;
    };
    match package.get("publish") {
        None => {
            let mut registries = toml_edit::Array::new();
            registries.push(slug);
            package.insert("publish", value(registries));
            Publish::Restricted
        }
        Some(item) => match item.as_value() {
            Some(toml_edit::Value::Boolean(b)) if !*b.value() => Publish::Unpublished,
            Some(toml_edit::Value::Array(list))
                if list.iter().any(|v| v.as_str() == Some(slug)) =>
            {
                Publish::Unchanged
            }
            // `publish.workspace = true`: the workspace decides.
            None => Publish::Unchanged,
            _ => Publish::Elsewhere,
        },
    }
}

/// Whether the manifest's crate is published to the registry: `publish` lists it, or is inherited from the workspace.
fn publishes_here(doc: &DocumentMut, slug: &str) -> bool {
    match doc.get("package").and_then(|p| p.get("publish")) {
        Some(item) => match item.as_value() {
            Some(toml_edit::Value::Array(list)) => list.iter().any(|v| v.as_str() == Some(slug)),
            Some(_) => false,
            None => true,
        },
        None => false,
    }
}

/// A path dependency [`pin_path_dependencies`] gave a version.
#[derive(Debug, PartialEq, Eq)]
pub struct Pinned {
    /// The workspace crate depended on.
    pub member: String,
    pub detail: String,
}

/// Gives path dependencies on crates of this workspace the `version` and `registry` that publishing needs: Cargo
/// drops `path` when it publishes, so the published crate names its dependency by version, in the registry. Covers
/// `[dependencies]`, `[build-dependencies]`, their `[target.*]` forms and `[workspace.dependencies]`; not
/// dev-dependencies, which Cargo publishes without them. Returns what changed.
pub fn pin_path_dependencies(
    doc: &mut DocumentMut,
    members: &BTreeMap<String, String>,
    slug: &str,
) -> Vec<Pinned> {
    let mut changes = Vec::new();
    let mut pin = |table: &mut dyn toml_edit::TableLike| {
        for (key, item) in table.iter_mut() {
            let Some(dep) = item.as_table_like_mut() else {
                continue;
            };
            if dep.get("path").is_none() || dep.get("version").is_some() {
                continue;
            }
            let name = dep
                .get("package")
                .and_then(Item::as_str)
                .unwrap_or(key.get())
                .to_owned();
            let Some(version) = members.get(&name) else {
                continue;
            };
            dep.insert("version", value(version.as_str()));
            let mut detail = format!("{key}: version = \"{version}\"");
            if dep.get("registry").is_none() {
                dep.insert("registry", value(slug));
                detail.push_str(&format!(", registry = \"{slug}\""));
            }
            if let Some(inline) = item.as_inline_table_mut() {
                // New keys go after the last value, and would inherit the space before `}`.
                inline.fmt();
            }
            changes.push(Pinned {
                member: name,
                detail,
            });
        }
    };
    for name in ["dependencies", "build-dependencies"] {
        if let Some(table) = doc.get_mut(name).and_then(Item::as_table_like_mut) {
            pin(table);
        }
    }
    if let Some(targets) = doc.get_mut("target").and_then(Item::as_table_like_mut) {
        for (_, target) in targets.iter_mut() {
            for name in ["dependencies", "build-dependencies"] {
                if let Some(table) = target.get_mut(name).and_then(Item::as_table_like_mut) {
                    pin(table);
                }
            }
        }
    }
    if let Some(table) = doc
        .get_mut("workspace")
        .and_then(|w| w.get_mut("dependencies"))
        .and_then(Item::as_table_like_mut)
    {
        pin(table);
    }
    changes
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
    let defaults = working_directory
        .map(|dir| format!("    defaults:\n      run:\n        working-directory: {dir}\n"))
        .unwrap_or_default();
    let (about, tags, publish) = if workspace {
        (
            format!(
                "# Publishes to the PrivateCrates registry `{slug}` from tags: story_engine-v1.2.3 publishes that one crate
# (the release-plz and cargo-release convention); v1.2.3 publishes every crate in the workspace, each of which needs
# a new version."
            ),
            r#"["v*", "*-v*"]"#,
            // The tag comes in through the environment, never pasted into the script.
            format!(
                "      - name: cargo publish
        env:
          TAG: ${{{{ github.ref_name }}}}
        run: |
          case \"$TAG\" in
            v[0-9]*) cargo publish --workspace --registry {slug} ;;
            *-v[0-9]*) cargo publish --package \"${{TAG%-v*}}\" --registry {slug} ;;
            *) echo \"::error::$TAG is neither v<version> nor <crate>-v<version>\"; exit 1 ;;
          esac
"
            ),
        )
    } else {
        (
            format!(
                "# Publishes to the PrivateCrates registry `{slug}` when a tag such as v1.2.3 is pushed."
            ),
            r#"["v*"]"#,
            format!("      - run: cargo publish --registry {slug}\n"),
        )
    };
    let install = privatecrates_common::install::ci_step(PROVIDER);
    format!(
        "{about} The job's OIDC token
# is the credential and the provenance: no secrets. Written by `cargo privatecrates init`.
name: publish
on:
  push:
    tags: {tags}
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-24.04
{defaults}    steps:
      - uses: actions/checkout@v5
{install}{publish}"
    )
}

/// Writes the workflow, unless an existing one already publishes to the registry or `force` is not given.
fn write_workflow(
    staged: &mut Staged,
    path: &Path,
    content: &str,
    slug: &str,
    force: bool,
) -> Result<Change, Error> {
    let change = |action, detail: &str| Change {
        path: path.to_owned(),
        action,
        detail: detail.to_owned(),
        workflow: true,
    };
    let existing = staged.read(path)?;
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
    staged.write(path, content.to_owned());
    let tags = if content.contains("*-v*") {
        "publishes on tags <crate>-v* (one crate) and v* (the workspace)"
    } else {
        "publishes on tags v*"
    };
    Ok(change(action, tags))
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
             # Needs cargo-credential-privatecrates (cargo binstall or cargo install it); the first build signs you in on GitHub, as does `cargo privatecrates login`.\n\
             # Joining the team, editors and troubleshooting: https://acme.privatecrates.dev/login\n\
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
            "# Needs cargo-credential-privatecrates (cargo binstall or cargo install it); the first build signs you in on GitHub, as does `cargo privatecrates login`.\n\
             # Joining the team, editors and troubleshooting: https://acme.privatecrates.dev/login\n\
             [registries.acme]\n\
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
    fn path_dependencies_on_workspace_crates_get_a_version_and_the_registry() {
        let members = BTreeMap::from([
            ("story_engine".to_owned(), "0.3.0".to_owned()),
            ("story_pack".to_owned(), "0.3.0".to_owned()),
        ]);
        let mut pack = doc(
            "[package]\nname = \"story_pack\"\npublish = [\"acme\"]\n\n[dependencies]\n\
             story_engine = { path = \"../engine\" }\n\
             serde = \"1\"\n\
             pinned = { path = \"../pinned\", version = \"1\" }\n\
             elsewhere = { path = \"../elsewhere\" }\n\n\
             [dev-dependencies]\nstory_engine = { path = \"../engine\" }\n\n\
             [target.'cfg(unix)'.build-dependencies]\nengine = { path = \"../engine\", package = \"story_engine\" }\n",
        );
        let changes: Vec<String> = pin_path_dependencies(&mut pack, &members, "acme")
            .into_iter()
            .map(|p| p.detail)
            .collect();
        assert_eq!(
            changes,
            [
                "story_engine: version = \"0.3.0\", registry = \"acme\"",
                "engine: version = \"0.3.0\", registry = \"acme\"",
            ]
        );
        let text = pack.to_string();
        assert!(
            text.contains("story_engine = { path = \"../engine\", version = \"0.3.0\", registry = \"acme\" }\nserde = \"1\"\n"),
            "{text}"
        );
        // Dev-dependencies, crates outside the workspace and versioned paths are left alone.
        assert!(
            text.contains("[dev-dependencies]\nstory_engine = { path = \"../engine\" }\n"),
            "{text}"
        );
        assert!(
            text.contains("elsewhere = { path = \"../elsewhere\" }\n"),
            "{text}"
        );
        assert!(pin_path_dependencies(&mut pack, &members, "acme").is_empty());

        let mut root = doc(
            "[workspace]\n[workspace.dependencies]\nstory_engine = { path = \"engine\", registry = \"acme\" }\n",
        );
        assert_eq!(
            pin_path_dependencies(&mut root, &members, "acme"),
            [Pinned {
                member: "story_engine".into(),
                detail: "story_engine: version = \"0.3.0\"".into()
            }]
        );
    }

    #[test]
    fn restricts_publish_to_the_registry() {
        let mut unset = doc("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
        assert_eq!(restrict_publish(&mut unset, "acme"), Publish::Restricted);
        assert!(unset.to_string().ends_with("publish = [\"acme\"]\n"));
        assert_eq!(restrict_publish(&mut unset, "acme"), Publish::Unchanged);
        let mut private = doc("[package]\nname = \"x\"\npublish = false\n");
        assert_eq!(restrict_publish(&mut private, "acme"), Publish::Unpublished);
        let mut public = doc("[package]\nname = \"x\"\npublish = true\n");
        assert_eq!(restrict_publish(&mut public, "acme"), Publish::Elsewhere);
        let mut other = doc("[package]\nname = \"x\"\npublish = [\"crates-io\"]\n");
        assert_eq!(restrict_publish(&mut other, "acme"), Publish::Elsewhere);
        let mut inherited = doc("[package]\nname = \"x\"\npublish.workspace = true\n");
        assert_eq!(restrict_publish(&mut inherited, "acme"), Publish::Unchanged);
    }

    #[test]
    fn replaces_a_workflow_only_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".github/workflows/publish.yml");
        let content = workflow("acme", false, None);
        // Plans, and applies the plan, as `init` does once the person agrees.
        let write = |force| {
            let mut staged = Staged::default();
            let change = write_workflow(&mut staged, &path, &content, "acme", force)?;
            staged.apply()?;
            Ok::<_, Error>(change.action)
        };
        assert_eq!(write(false).unwrap(), Action::Created);
        assert_eq!(write(false).unwrap(), Action::Unchanged);

        // Edited, e.g. to pin the runner: still publishes to the registry, so it is kept.
        let pinned = content.replace("ubuntu-24.04", "ubuntu-22.04");
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
            format!("# Publishes to the PrivateCrates registry `acme` when a tag such as v1.2.3 is pushed. The job's OIDC token
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
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v5
{}      - run: cargo publish --registry acme
",
                privatecrates_common::install::ci_step(PROVIDER)
            )
        );
        let workspace = workflow("acme", true, Some("rust"));
        assert!(
            workspace.contains("    tags: [\"v*\", \"*-v*\"]\n"),
            "{workspace}"
        );
        assert!(
            workspace.contains("          TAG: ${{ github.ref_name }}\n"),
            "{workspace}"
        );
        assert!(workspace.contains("v[0-9]*) cargo publish --workspace --registry acme ;;"));
        assert!(
            workspace
                .contains("*-v[0-9]*) cargo publish --package \"${TAG%-v*}\" --registry acme ;;")
        );
        assert!(workspace.contains(
            "    runs-on: ubuntu-24.04\n    defaults:\n      run:\n        working-directory: rust\n    steps:\n"
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
