<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import {
		cargoConfig,
		dependency,
		installProvider,
		loginCommands,
		publishTarget,
		settingsToml
	} from '$lib/snippets';
	import { FREE_MEMBER_LIMIT, PREVIEW, TERMS_PATH, TRIAL_MONTHS } from '$lib/site';
</script>

<Seo
	title="Set up a registry"
	description="Install the PrivateCrates GitHub Apps, create the storage repository, choose a registry name and configure Cargo with the credential provider."
	path="/docs/setup"
/>

<h1>Set up a registry</h1>
<p class="lede">
	About ten minutes for an organisation admin, then one config file and one install for each developer.
</p>

<Callout title="Works with AI coding agents">
	<p>
		A coding agent such as Claude Code can do most of this for you with the <code>cargo privatecrates</code>
		CLI, and stops to give you a link where GitHub needs a person. See
		<a href="/docs/agents">Set up with an AI agent</a>.
	</p>
</Callout>

<h2 id="organisation">For the organisation</h2>
<p>
	Sign in on the <a href="/account">account page</a> with GitHub and pick your organisation. The checklist there
	walks through these steps and ticks each one off as GitHub reports it done. Only organisation admins can complete
	them.
</p>
<p>
	A personal GitHub account can have a registry too: it appears on the account page once the reader App is
	installed on it, you are its only admin, and it is always free.
</p>
<ol>
	<li>
		<strong>Install the reader App</strong> on all repositories, or on those that own crates. It can read repository
		metadata and organisation membership, nothing else. PrivateCrates uses it to sign developers in and to ask GitHub
		what each person can access.
	</li>
	<li>
		<strong>Choose the storage repository</strong>: a new private repository, conventionally
		<code>crates-store</code>, or an existing empty one. It holds your registry and nothing else: the index,
		and every crate file as a release. Enable <strong>immutable releases</strong> in Settings → General → Releases.
		Immutability means that once a version is published, nobody (including us) can change its bytes.
	</li>
	<li>
		<strong>Install the storage App</strong> on that one repository only. It has contents write there, which it
		uses to add releases and index files.
	</li>
	<li>
		<strong>Choose your registry name.</strong> It becomes your hostname,
		<code>your-name.privatecrates.dev</code>, and is saved as <code>privatecrates.toml</code> in the storage
		repository. In the same step you accept the <a href={TERMS_PATH}>preview terms</a> on behalf of your organisation.
	</li>
	<li>
		{#if PREVIEW}
			<strong>Plan.</strong> Nothing to choose: every organisation is free during the preview. See
			<a href="/pricing">pricing</a> for what is planned from general availability.
		{:else}
			<strong>Choose a plan.</strong> Organisations with {FREE_MEMBER_LIMIT} or fewer members are free, with nothing
			to choose. Larger ones start a {TRIAL_MONTHS}-month free trial with one click and no card; see
			<a href="/pricing">pricing</a>.
		{/if}
	</li>
</ol>

<h3 id="settings-file">The settings file</h3>
<p>
	Settings live in your repository, not in our database, so you change them with a pull request and the git
	history is the audit log.
</p>
<CodeBlock caption="privatecrates.toml" code={settingsToml('acme')} />
<ul>
	<li>
		<code>name_clash</code>: publishing a crate whose name also exists on crates.io is refused by default,
		because a developer who forgets <code>registry = "acme"</code> would get the public crate.
	</li>
	<li>
		<code>ci_read</code>: with <code>"organisation"</code>, any workflow in the organisation can read every
		crate. A stricter <code>"same-access"</code>, reading only the crates whose owning repository the
		workflow’s own repository could read, is planned.
	</li>
	<li>
		<code>allow_manual_publish</code>: whether crates may also be published from developers’ machines, first
		versions included (you choose when you create the registry). It is the default for every repository; a
		<code>[repositories.&lt;name&gt;]</code> table overrides it for one. See
		<a href="/docs/publishing#laptop">Publishing from a developer’s machine</a>.
	</li>
</ul>

<h2 id="developers">For each developer</h2>
<h3>1. Install the credential provider</h3>
<p>
	<code>cargo-credential-privatecrates</code> is open source and implements Cargo’s credential provider protocol,
	so every Cargo command works unchanged.
</p>
<CodeBlock caption="shell" code={installProvider} />

<h3>2. Add the registry</h3>
<p>
	Commit this to the repository of every project that uses private crates, or put it in
	<code>~/.cargo/config.toml</code>.
</p>
<CodeBlock caption=".cargo/config.toml" code={cargoConfig('acme', 'https://acme.privatecrates.dev')} />

<h3>3. Depend on private crates</h3>
<CodeBlock caption="Cargo.toml" code={dependency('acme')} />
<p>
	The first time Cargo needs a token, the provider prints a code and opens GitHub’s device sign-in. Approve it
	in the browser and you are done: the refresh token goes into your operating system’s keyring, and the 8-hour
	access token is refreshed silently from then on.
</p>
<CodeBlock caption="shell" code={loginCommands('acme')} />

<Callout title="What that token can do">
	<p>
		The token comes from the reader App, so it can list repository metadata and nothing more. If it were
		stolen, it could not read code, crates or anything else.
	</p>
</Callout>

<h3>4. Make crates publishable</h3>
<p>
	Set <code>publish</code> so a crate can only ever go to your registry, and <code>repository</code> so its first
	publish knows which repository owns it.
</p>
<CodeBlock caption="Cargo.toml" code={publishTarget('acme')} />
<p>Then <a href="/docs/ci">set up CI</a> and <a href="/docs/publishing">publish from a tag</a>.</p>

<h2 id="other-tokens">Using gh or a personal access token instead</h2>
<p>
	Tokens from <code>gh auth token</code>, and classic or fine-grained personal access tokens, also work
	through Cargo’s <code>cargo:token-from-stdout</code> provider. We recommend the credential provider instead, because
	those tokens are usually far broader than the registry needs.
</p>

<h2 id="sso">SAML single sign-on</h2>
<p>
	If your organisation enforces SAML SSO, GitHub requires an active SSO session for the token. When it is
	missing, the registry answers with an error saying so and the URL to authorise, rather than a confusing “not
	found”.
</p>
