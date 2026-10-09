<script lang="ts">
	import type { Tenant } from '$lib/api';
	import AgentPrompt from '$lib/components/AgentPrompt.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import LoadingDock from '$lib/illustrations/LoadingDock.svelte';
	import {
		cargoConfig,
		ciPublish,
		dependency,
		installProvider,
		publishPrompt,
		publishTarget,
		pushTag
	} from '$lib/snippets';
	import { baseDomain } from './format';

	interface Props {
		org: string;
		tenant: Tenant;
	}

	let { org, tenant }: Props = $props();

	// The crate on the crane shows the registry name when it fits the stencil.
	const crateLabel = $derived(tenant.slug.length <= 7 ? tenant.slug : undefined);

	const agentPrompt = $derived(publishPrompt({ org, slug: tenant.slug, apex: baseDomain() }));

	// The URL may wrap only after the scheme and before each dot, never inside a name: "worldbuilding-dev" /
	// ".privatecrates.dev", not "worldbuilding-dev.privatecrat" / "es.dev".
	const urlParts = $derived.by(() => {
		const url = tenant.registry_url ?? '';
		const [scheme, host] = url.includes('://') ? url.split('://', 2) : ['', url];
		const labels = host.split('.');
		return {
			scheme: scheme ? `${scheme}://` : '',
			labels: labels.map((label, i) => (i === 0 ? label : `.${label}`))
		};
	});
</script>

<section class="ready" aria-labelledby="ready-{org}">
	<div class="welcome">
		<div>
			<h3 id="ready-{org}">Your registry is ready</h3>
			<p class="url">
				<a href={tenant.registry_url}
					>{urlParts.scheme}<wbr />{#each urlParts.labels as label, i (i)}{#if i > 0}<wbr
							/>{/if}{label}{/each}</a
				>
			</p>
			<p class="intro">
				Five steps take a crate from a repository in <strong>{org}</strong> to a build that depends on it.
				Each snippet below is already filled in for <code>{tenant.slug}</code>.
			</p>
		</div>
		<LoadingDock class="welcome-art" label={crateLabel} />
	</div>

	<AgentPrompt id="agent-{org}" title="Set up with your AI agent" prompt={agentPrompt} primary>
		<p>
			A coding agent such as Claude Code can do the steps below across {org}’s repositories: it runs
			<code>cargo privatecrates init</code> in each one, opens pull requests for you to review, publishes a
			first version by pushing a tag, and checks the result with <code>cargo privatecrates doctor</code>.
		</p>
		<p><a href="/docs/agents">What the agent does, step by step</a></p>
	</AgentPrompt>

	<ol class="guide">
		<li>
			<h4>Install the credential provider</h4>
			<p>
				Each developer installs it once. It signs in with GitHub when Cargo needs a token, so there are no
				registry tokens to hand out.
			</p>
			<CodeBlock caption="shell" code={installProvider} />
			<p class="docs"><a href="/docs/setup#developers">How the provider signs in</a></p>
		</li>
		<li>
			<h4>Add the registry to a repository</h4>
			<p>Commit this to each repository that publishes or uses private crates.</p>
			<CodeBlock caption=".cargo/config.toml" code={cargoConfig(tenant.slug, tenant.registry_url)} />
			<p class="docs"><a href="/docs/setup">Set-up guide</a></p>
		</li>
		<li>
			<h4>Publish a first crate from GitHub Actions</h4>
			<p>
				Set <code>repository</code> to the crate’s GitHub repository, and <code>publish</code> so it can only go
				to your registry.
			</p>
			<CodeBlock caption="Cargo.toml" code={publishTarget(tenant.slug, org)} />
			<p>
				Add the publish workflow. It runs on a version tag and publishes with GitHub’s OIDC token, so the
				repository needs no secrets.
			</p>
			<CodeBlock caption=".github/workflows/publish.yml" code={ciPublish(tenant.slug)} />
			<p>Then push a tag.</p>
			<CodeBlock caption="shell" code={pushTag} />
			<p class="docs">
				<a href="/docs/publishing">Publishing guide</a> and <a href="/docs/ci">CI without secrets</a>
			</p>
		</li>
		<li>
			<h4>Depend on it</h4>
			<p>
				The first build asks the developer to approve a GitHub sign-in in the browser. After that, Cargo
				refreshes the token by itself.
			</p>
			<CodeBlock caption="Cargo.toml" code={dependency(tenant.slug)} />
		</li>
		<li>
			<h4>Verify the registry <span class="optional">(optional)</span></h4>
			<p>
				Run the open-source verifier in your storage repository’s CI, on every publish and daily. It checks
				every crate and index entry we write against GitHub’s own records.
			</p>
			<p class="docs"><a href="/docs/verify">Set up the verifier</a></p>
		</li>
	</ol>
</section>

<style>
	.ready {
		display: grid;
		gap: 1.75rem;
	}
	.welcome {
		display: grid;
		gap: 1.25rem;
		align-items: end;
		padding-bottom: 1.5rem;
		border-bottom: 2px solid var(--line);
	}
	@media (min-width: 44rem) {
		.welcome {
			grid-template-columns: minmax(0, 1fr) 19rem;
		}
	}
	h3 {
		font-size: var(--text-3xl);
	}
	.url {
		font-family: var(--font-mono);
		font-size: var(--text-base);
		/* Breaks only at the <wbr>s; a single name longer than the column still wraps rather than overflowing. */
		overflow-wrap: break-word;
		margin-top: 0.5rem;
	}
	.intro {
		margin-top: 0.75rem;
		color: var(--ink-muted);
		max-width: 36rem;
	}
	.welcome :global(.welcome-art) {
		width: 100%;
		max-width: 20rem;
		height: auto;
		justify-self: center;
	}

	/* A real sequence: the counters are the step numbers. */
	.guide {
		counter-reset: step;
		display: grid;
		gap: 2rem;
	}
	.guide > li {
		counter-increment: step;
		position: relative;
		display: grid;
		gap: 0.75rem;
		padding-left: 3rem;
		min-width: 0;
	}
	.guide > li::before {
		content: counter(step);
		position: absolute;
		left: 0;
		top: -0.1rem;
		display: grid;
		place-items: center;
		width: 2.1rem;
		height: 2.1rem;
		border: 2px solid var(--line);
		border-radius: 4px;
		background: var(--primary);
		color: var(--primary-ink);
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-lg);
	}
	/* The rope between crates: a line from each step's marker down to the next. */
	.guide > li:not(:last-child)::after {
		content: '';
		position: absolute;
		left: calc(1.05rem - 1px);
		top: 2.25rem;
		bottom: -1.9rem;
		border-left: 2px dashed var(--divider);
	}
	h4 {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		line-height: 1.1;
	}
	.optional {
		font-family: var(--font-sans);
		font-weight: 400;
		font-size: var(--text-base);
		color: var(--ink-muted);
	}
	.guide p {
		max-width: 40rem;
	}
	.docs {
		font-size: var(--text-sm);
	}
</style>
