<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { ciPublish, ciPublishWorkspace, ownersToml, publishRefused, publishTarget } from '$lib/snippets';

	const manualPublishSettings = `# The default for every repository
allow_manual_publish = false

# Except this one
[repositories.tools]
allow_manual_publish = true`;

	const yank = `cargo yank --registry acme --version 0.2.0 story_engine
cargo yank --registry acme --version 0.2.0 --undo story_engine`;
</script>

<Seo
	title="Publishing"
	description="Trusted publishing from GitHub Actions with plain cargo publish: tokens bound to the exact crate bytes, owners files, first publishes, yanking and manual publishing."
	path="/docs/publishing"
/>

<h1>Publishing</h1>
<p class="lede">
	Publishing is plain <code>cargo publish --registry acme</code>. What matters is where it runs: from GitHub
	Actions, every version gets provenance you can verify.
</p>

<h2 id="trusted">Trusted publishing from GitHub Actions</h2>
<p>Add a workflow that publishes when you push a version tag:</p>
<CodeBlock caption=".github/workflows/publish.yml" code={ciPublish('acme')} />
<p>What happens when it runs:</p>
<ol>
	<li>
		Cargo packages the crate and asks the credential provider for a publish token, passing the crate’s name,
		version and SHA-256 checksum.
	</li>
	<li>
		The provider requests an Actions OIDC token whose audience binds it to exactly those:
		<code>https://acme.privatecrates.dev/publish/story_engine/0.2.0/&lt;sha256&gt;</code>. It is used once and
		never cached.
	</li>
	<li>
		The registry accepts the upload only if the checksum of the bytes it received matches the audience, the
		job ran in the crate’s owning repository, and its workflow (and environment, if required) is allowed by
		the crate’s owners file.
	</li>
	<li>
		The crate is stored as an immutable GitHub release in your storage repository, together with the OIDC
		token as <code>story_engine-0.2.0.provenance.jwt</code>: a statement signed by GitHub, not by us, that
		this workflow run published exactly these bytes.
	</li>
</ol>
<p>Because the token names one version and one checksum, it cannot be replayed to publish anything else.</p>

<h3 id="releasing">Releasing a version</h3>
<p>
	Bump <code>version</code> in <code>Cargo.toml</code>, let Cargo update <code>Cargo.lock</code> (any build
	does, or <code>cargo update --workspace</code>), and commit both before pushing the tag. If
	<code>Cargo.lock</code>
	is committed but still names the old version, <code>cargo publish</code> in CI updates it, finds the
	checkout changed and stops with
	<em>1 files in the working directory contain changes that were not yet committed into git: Cargo.lock</em>.
	<code>cargo privatecrates doctor</code> catches this before you tag. A failed run publishes nothing, so commit
	the lockfile and push a tag for the next version.
</p>

<h2 id="workspaces">Workspaces and monorepos</h2>
<p>
	Many crates can live in one repository: each crate’s <code>package.repository</code> names that repository
	(a link to the crate’s directory, such as <code>https://github.com/acme/mono/tree/main/crates/foo</code>,
	also works), and each gets its own owners file. Cargo asks for one token per crate.
</p>
<p>
	Crates in a workspace usually get their own tags. In a workspace, <code>cargo privatecrates init</code>
	writes a workflow that follows the release-plz and cargo-release convention:
	<code>story_engine-v0.2.0</code>
	publishes just <code>story_engine</code>, and <code>v0.2.0</code> publishes every crate in the workspace, each
	of which needs a new version.
</p>
<p>
	When crates depend on each other, one <code>v0.2.0</code> tag is simplest: Cargo publishes them in
	dependency order, waiting until each is in the registry before publishing the crates that need it. With
	per-crate tags, push the tag of a crate others depend on first, and wait for its run to finish. A dependency
	on another crate of the workspace needs a <code>version</code> and the <code>registry</code> besides its
	<code>path</code>, because Cargo drops the path when it publishes; <code>cargo privatecrates init</code> adds
	them.
</p>
<p>
	A tag can be used only once. If the repository already has a <code>v0.2.0</code> tag, for example from when
	its crates were git dependencies, use per-crate tags or a new version;
	<code>cargo privatecrates doctor</code>
	suggests a tag that is free.
</p>
<CodeBlock caption=".github/workflows/publish.yml" code={ciPublishWorkspace('acme')} />
<p>
	The tag reaches the script through an environment variable rather than being pasted into it, so a crafted
	tag name cannot run commands.
</p>

<h2 id="first-publish">A crate’s first publish</h2>
<p>
	A new crate is first published from CI too, so its first version always has provenance. Its
	<code>Cargo.toml</code> must name a repository in your organisation, and the workflow must run in that repository:
</p>
<CodeBlock caption="Cargo.toml" code={publishTarget('acme')} />
<p>
	The registry then creates the crate’s owners file, with that repository as the owner and the publishing
	workflow as the only one allowed.
</p>

<h2 id="owners">The owners file</h2>
<p>
	Each crate has <code>owners/&lt;name&gt;.toml</code> in the storage repository. Changing who may publish is an
	administrator’s edit to this file, by pull request; there is deliberately no API for it.
</p>
<CodeBlock caption="owners/story_engine.toml" code={ownersToml} />
<ul>
	<li>
		<code>repository_id</code> is authoritative, so renaming or transferring the repository inside the organisation
		breaks nothing.
	</li>
	<li>
		<code>publish_environment</code> requires the job to run in a GitHub environment, so its protection rules apply:
		a required reviewer gives you a two-person rule, enforced by GitHub.
	</li>
</ul>

<h2 id="laptop">Publishing from a developer’s machine</h2>
<p>
	When you create the registry you choose whether developers may run <code>cargo publish</code> from their own machines.
	Unless you allow it, it is refused, with instructions:
</p>
<CodeBlock caption="cargo publish output" code={publishRefused} hashComments={false} />
<p>
	The choice is <code>allow_manual_publish</code> in <code>privatecrates.toml</code>: the default for every
	repository, which a repository’s own <code>[repositories.&lt;name&gt;]</code> table overrides, both ways. A crate’s
	owners file can also allow it for that one crate. Change any of them with a pull request.
</p>
<CodeBlock caption="privatecrates.toml" code={manualPublishSettings} />
<p>Where it is allowed, a publish from a machine is accepted when:</p>
<ul>
	<li>
		the publisher has write access to the crate’s repository (for a first version, the repository its
		<code>package.repository</code> names, in your organisation), and
	</li>
	<li>
		the crate was packaged from a clean git checkout: Cargo records the commit in the package, and we refuse
		packages without one or with uncommitted changes (<code>--allow-dirty</code>).
	</li>
</ul>
<p>
	The commit is the publisher’s claim, not something GitHub signed. These versions have no provenance, and the
	verifier, search and the compliance dashboard show them as manual publishes, with who published them and
	from which commit. A crate first published from a machine can later be published from any workflow in its
	repository; add <code>publish_workflows</code> to its owners file to narrow that.
</p>

<h2 id="yank">Yanking</h2>
<p>
	Anyone with push access to the owning repository can yank or unyank a version from their machine, without
	provenance: yanking has to be quick, changes no bytes and can be undone.
</p>
<CodeBlock caption="shell" code={yank} />

<Callout title="Versions are permanent">
	<p>
		Every version is kept forever: Cargo needs yanked versions to stay downloadable for existing lockfiles,
		and immutable releases make the history tamper-proof. A version number can never be reused, so fix a bad
		release by publishing a new version.
	</p>
</Callout>

<h3 id="remove">Removing a version in an emergency</h3>
<p>
	Yanking stops new builds choosing a version but leaves it downloadable. If a published crate contains
	something that must not be downloaded, such as a secret:
</p>
<ol>
	<li>Rotate the secret. Anyone who downloaded the version already has it; removal does not change that.</li>
	<li>Yank the version, so no new build picks it.</li>
	<li>
		An organisation admin deletes that version’s release in the storage repository on GitHub. Immutable
		releases cannot be changed, but an admin can delete one; PrivateCrates’ Apps cannot. Builds whose
		<code>Cargo.lock</code> pins the version then fail to download it, and its version number can never be used
		again.
	</li>
	<li>
		Expect <a href="/docs/verify">the verifier</a> to report the missing release: it is a deliberate, audited removal,
		and the report records it.
	</li>
</ol>

<h2 id="clashes">Names that exist on crates.io</h2>
<p>
	A private crate whose name also exists on crates.io is a dependency-confusion risk. Publishing such a name
	is refused unless <code>name_clash = "warn"</code> is set in <code>privatecrates.toml</code>. Someone could
	also claim your crate’s name on crates.io later, so the registry checks daily and the verifier reports
	clashes. For names that matter, reserve them on crates.io with a placeholder crate.
</p>
