<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo
	title="Documentation"
	description="How PrivateCrates works: a Cargo registry whose storage, identity and permissions are your GitHub organisation's."
	path="/docs"
/>

<h1>Documentation</h1>
<p class="lede">
	PrivateCrates implements Cargo’s sparse registry protocol in front of your GitHub organisation. GitHub stays
	the store, the identity provider and the source of truth for permissions.
</p>

<h2>The model in one paragraph</h2>
<p>
	Crate files are stored as GitHub release assets in a private repository your organisation owns, and the
	registry index lives in the same repository. Each crate has an owning repository, usually the one its code
	is in. Anyone who can read that repository on GitHub can use the crate; anyone who can push to it can
	publish it, from CI. There are no separate accounts, keys or access lists.
</p>

<h2>Guides</h2>
<ul class="guide-list">
	<li>
		<a href="/docs/setup">Set up a registry</a>: install the Apps, create the storage repository, choose a
		name, and configure Cargo on developers’ machines.
	</li>
	<li>
		<a href="/docs/agents">Set up with an AI agent</a>: let Claude Code or another coding agent set up the
		registry and configure your crates, with the <code>cargo privatecrates</code> CLI and your own
		<code>gh</code> login.
	</li>
	<li>
		<a href="/docs/ci">CI without secrets</a>: read crates from GitHub Actions with OIDC, and build Docker
		images or use other CI safely.
	</li>
	<li>
		<a href="/docs/publishing">Publishing</a>: trusted publishing from Actions, the owners file, first
		publishes, yanking and publishing from a laptop.
	</li>
	<li>
		<a href="/docs/verify">Verify the registry</a>: run <code>privatecrates-verify</code> on a schedule to check
		everything PrivateCrates writes.
	</li>
	<li>
		<a href="/docs/security">Security model</a>: what we see, what we store, and what happens if a token, a
		key or the service itself is compromised.
	</li>
	<li>
		<a href="/docs/errors">Error reference</a>: every error code the registry returns, and what to do about
		it.
	</li>
</ul>

<h2>Names used in examples</h2>
<p>
	The examples use an organisation called <code>acme</code>, whose registry is also named <code>acme</code>
	and served at <code>https://acme.privatecrates.dev</code>, and a crate called <code>story_engine</code>
	owned by the repository <code>acme/story-engine</code>. Replace them with your own; the account page shows
	your registry’s exact configuration once it is live.
</p>

<style>
	.guide-list {
		list-style: none !important;
		padding-left: 0 !important;
	}
	.guide-list li {
		padding: 0.75rem 0;
		border-bottom: 1px solid var(--rule);
		margin-top: 0 !important;
	}
	.guide-list a {
		font-weight: 700;
	}
</style>
