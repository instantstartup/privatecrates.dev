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

<h2 id="organisation">For the organisation</h2>
<p>
	Sign in on the <a href="/account">account page</a> with GitHub and pick your organisation. The checklist there
	walks through these steps and ticks each one off as GitHub reports it done. Only organisation admins can complete
	them.
</p>
<ol>
	<li>
		<strong>Install the reader App</strong> on all repositories, or on those that own crates. It can read repository
		metadata and organisation membership, nothing else. PrivateCrates uses it to sign developers in and to ask GitHub
		what each person can access.
	</li>
	<li>
		<strong>Create the storage repository</strong>: a private repository, conventionally
		<code>crates-store</code>, with <strong>immutable releases</strong> enabled in Settings → General → Releases.
		Immutability means that once a version is published, nobody (including us) can change its bytes.
	</li>
	<li>
		<strong>Install the storage App</strong> on that one repository only. It has contents write there, which it
		uses to add releases and index files.
	</li>
	<li>
		<strong>Choose your registry name.</strong> It becomes your hostname,
		<code>your-name.privatecrates.dev</code>, and is saved as <code>privatecrates.toml</code> in the storage repository.
	</li>
	<li><strong>Start the 14-day trial</strong> through Stripe Checkout.</li>
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
		crate. With <code>"same-access"</code>, a workflow can read only the crates whose owning repository its
		own repository could read.
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
