<script lang="ts">
	import AgentPrompt from '$lib/components/AgentPrompt.svelte';
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { EXAMPLE_SLUG } from '$lib/site';
	import { cargoConfig, cli, installProvider, joinPrompt, PROD_APEX } from '$lib/snippets';

	const slug = EXAMPLE_SLUG;
	const registry = `https://${slug}.${PROD_APEX}`;
</script>

<Seo
	title="Joining a team"
	description="For a developer on a project that already uses a PrivateCrates registry: install the credential provider, sign in once with GitHub, and fix “not found”."
	path="/docs/joining"
/>

<h1>Joining a team</h1>
<p class="lede">
	Your team already uses PrivateCrates. You need one install and one GitHub sign-in, and no account of your
	own.
</p>
<p>
	Access follows GitHub: you can use a private crate if you can read the repository it is published from. The
	examples use the registry <code>{slug}</code>; every registry also has its own page, at
	<a href="{registry}/login">{registry}/login</a>, with its names filled in.
</p>

<h2 id="install">Install and sign in</h2>
<CodeBlock caption="shell" code={installProvider} />
<p>
	Then build as usual. The first time Cargo needs the registry, the provider shows a code: approve it on
	GitHub and you are signed in for every project using your organisation’s registry. To sign in first, for
	example before opening the project in an editor:
</p>
<CodeBlock caption="shell" code={`cargo login --registry ${slug}`} />
<p>
	The project’s <code>.cargo/config.toml</code> should already name the registry. If it does not, add it (or ask
	whoever set the registry up to commit it):
</p>
<CodeBlock caption=".cargo/config.toml" code={cargoConfig(slug, registry)} />

<h2 id="editors">Editors and background builds</h2>
<p>
	Editors such as rust-analyzer run Cargo in the background, with no terminal to show a sign-in code. Rather
	than wait for an approval nobody can see, the build stops at once with <em
		>not signed in … run
		<code>cargo login --registry {slug}</code> in a terminal</em
	>. Run that once, then reload the editor. The same applies to coding agents.
</p>

<h2 id="not-found">A crate is “not found”</h2>
<p>
	The registry answers “not found” both for a crate that does not exist and for one published from a
	repository you cannot read. It does not say which, so the names of private crates stay private. To check
	your set-up and sign-in and look the crate up:
</p>
<CodeBlock caption="shell" code={cli('doctor --crate story_engine')} />
<p>
	If everything else passes, ask someone in your organisation for read access to the crate’s repository on
	GitHub (or check the name with whoever publishes it). Access changes take effect within a few minutes.
</p>

<h2 id="troubleshooting">Other problems</h2>
<ul>
	<li>
		<strong>Every crate is “not found”.</strong> Your GitHub account must be a member of the organisation. If it
		is, sign out and in again, and grant the PrivateCrates app access to the organisation when GitHub asks.
	</li>
	<li>
		<strong>Signed in as the wrong GitHub account.</strong> <code>cargo logout --registry {slug}</code>, then
		<code>cargo login --registry {slug}</code>.
	</li>
	<li>
		<strong>A version someone just published is missing.</strong> Run <code>cargo update</code>.
	</li>
	<li>
		<strong>An SSO error.</strong> Your organisation enforces SAML single sign-on: follow the link in the error
		to authorise, then retry.
	</li>
</ul>
<p>Error codes are explained in the <a href="/docs/errors">error reference</a>.</p>

<h2 id="agent">With a coding agent</h2>
<AgentPrompt
	id="join-prompt"
	title="Prompt: set up this project"
	prompt={joinPrompt({ slug, apex: PROD_APEX })}
>
	<p>
		Paste this into Claude Code or another coding agent in the project’s directory. It installs the provider,
		checks the configuration, and asks you to run the sign-in yourself: it never approves a sign-in for you.
	</p>
</AgentPrompt>

<Callout title="Publishing">
	<p>
		Crates are published from GitHub Actions, and only by people who can create releases in the crate’s
		repository. See <a href="/docs/publishing">Publishing</a>.
	</p>
</Callout>
