<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { FREE_MEMBER_LIMIT, PREVIEW, TERMS_PATH, TERMS_VERSION, TRIAL_MONTHS } from '$lib/site';
	import {
		cli,
		cliUsage,
		doctorCommands,
		initCommands,
		installCli,
		loginCli,
		PROD_APEX,
		publishPrompt,
		setupCommands,
		setupPrompt,
		STORAGE_REPO,
		storageRepoCommands
	} from '$lib/snippets';

	type Who = 'agent' | 'you' | 'both';
	const steps: { step: string; who: Who; how: string }[] = [
		{
			step: 'Sign in to PrivateCrates',
			who: 'both',
			how: 'The agent runs cargo privatecrates login and gives you a code and a link. You approve the sign-in on GitHub, once.'
		},
		{
			step: 'Install the reader App',
			who: 'you',
			how: 'GitHub has no API to install an App. The agent gives you the link; you install it on the organisation.'
		},
		{
			step: 'Create the storage repository',
			who: 'agent',
			how: `gh repo create, with your own gh login, as a private repository named ${STORAGE_REPO}.`
		},
		{
			step: 'Turn on immutable releases',
			who: 'agent',
			how: 'gh api, with your rights. Our Apps never get administration rights.'
		},
		{
			step: 'Install the storage App on that repository',
			who: 'you',
			how: 'The agent gives you the link, pre-selected for your organisation; you choose that one repository.'
		},
		{
			step: 'Accept the preview terms',
			who: 'you',
			how: 'The agent gives you the link to the terms and waits. You read them and tell it you accept, on behalf of your organisation. It never accepts for you.'
		},
		{
			step: 'Choose the registry name',
			who: 'agent',
			how: `cargo privatecrates setup --slug --accept-terms ${TERMS_VERSION}, only after you have accepted. The service saves privatecrates.toml in the storage repository.`
		},
		...(PREVIEW
			? []
			: [
					{
						step: `Start the trial (over ${FREE_MEMBER_LIMIT} members)`,
						who: 'agent' as Who,
						how: `cargo privatecrates setup --start-trial --billing-email: ${TRIAL_MONTHS} months, no card. The agent asks you for the billing email first.`
					}
				]),
		{
			step: 'Configure each crate repository',
			who: 'agent',
			how: 'cargo privatecrates init, then a pull request. You review and merge.'
		},
		{
			step: 'Publish a first version',
			who: 'agent',
			how: 'It pushes a tag; GitHub Actions publishes. cargo privatecrates doctor confirms the result.'
		}
	];
	const whoLabel: Record<Who, string> = { agent: 'Agent', you: 'You', both: 'Agent, you approve' };

	const org = 'acme';
	const examplePrompt = setupPrompt({
		org,
		slug: org,
		apex: PROD_APEX,
		steps: [
			{
				id: 'reader_app',
				status: 'todo',
				action_url: 'https://github.com/apps/privatecrates-reader/installations/new'
			},
			{ id: 'storage_repo', status: 'todo' },
			{
				id: 'storage_app',
				status: 'todo',
				action_url: 'https://github.com/apps/privatecrates-storage/installations/new'
			},
			{ id: 'settings', status: 'blocked' },
			{ id: 'plan', status: PREVIEW ? 'done' : 'todo' }
		],
		plan: PREVIEW ? null : 'trial',
		preview: PREVIEW
	});
	const examplePublish = publishPrompt({ org, slug: org, apex: PROD_APEX });
</script>

<Seo
	title="Set up with an AI agent"
	description="Let a coding agent such as Claude Code set up your PrivateCrates registry and configure your crates, with the cargo privatecrates CLI and your own gh login."
	path="/docs/agents"
/>

<h1>Set up with an AI agent</h1>
<p class="lede">
	Tell a coding agent such as Claude Code to set up PrivateCrates for your organisation and publish your
	crates. It does the work with the <code>cargo privatecrates</code> CLI and your own <code>gh</code> login, and
	stops only where GitHub insists on a person.
</p>

<h2 id="who">What the agent does, and what needs you</h2>
<p>
	You need to be an admin of the GitHub organisation, with the GitHub CLI signed in (<code
		>gh auth status</code
	>). Four moments need you: approving the agent’s sign-in, installing each of the two GitHub Apps, and
	accepting the <a href={TERMS_PATH}>preview terms</a> on behalf of your organisation.
</p>
<div class="table-scroll">
	<table>
		<thead>
			<tr><th scope="col">Step</th><th scope="col">Who</th><th scope="col">How</th></tr>
		</thead>
		<tbody>
			{#each steps as s (s.step)}
				<tr>
					<th scope="row">{s.step}</th>
					<td class={['who', s.who]}>{whoLabel[s.who]}</td>
					<td>{s.how}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>

<h2 id="prompts">The prompts</h2>
<p>
	The <a href="/account">account page</a> gives you both prompts filled in for your organisation, with its
	install links, under “Set up with your AI agent”. The examples below are for <code>acme</code>. Each prompt
	points the agent at <a href="/llms.txt" data-sveltekit-reload>/llms.txt</a>, which describes the whole flow
	for agents.
</p>
<h3 id="prompt-setup">1. Set up the registry</h3>
<p>
	The agent goes through the checklist in order, skipping what is done, and hands you a link at each step that
	needs you. It checks its progress with <code>{cli(`setup ${org} --json`)}</code>.
</p>
<CodeBlock caption="Prompt: set up the registry" code={examplePrompt} hashComments={false} />

<h3 id="prompt-publish">2. Configure and publish crates</h3>
<p>
	Once the registry is live, this prompt configures each crate repository you choose, opens pull requests for
	you to review, and publishes a first version from CI.
</p>
<CodeBlock caption="Prompt: configure and publish crates" code={examplePublish} hashComments={false} />

<h2 id="cli">The cargo privatecrates CLI</h2>
<p>
	An open-source Cargo subcommand for setting up a registry and configuring repositories. Agents do better
	with one deterministic command than with a recipe, and so do people.
</p>
<CodeBlock caption="shell" code={installCli} />
<CodeBlock caption="usage" code={cliUsage} />
<p>
	Every command talks to <code>{PROD_APEX}</code>. For another environment, add <code>--domain</code> with its domain
	to each command.
</p>

<h3 id="cli-login">login and logout</h3>
<p>
	Signs in with GitHub’s device flow for the reader App, the same sign-in and token store as the credential
	provider. The token can read repository metadata and nothing else.
</p>
<CodeBlock caption="shell" code={loginCli()} />

<h3 id="cli-setup">setup</h3>
<p>
	Prints the organisation’s set-up checklist, each step’s status, and a link where a person must act.
	<code>--slug</code> saves the registry name, and needs <code>--accept-terms</code> with the version of the
	terms an admin has read and accepted; without it, the command prints the terms’ link and the exact flag to
	add, and stops. {#if !PREVIEW}<code>--start-trial --billing-email &lt;address&gt;</code> starts the no-card trial
		(Stripe sends the trial-ending reminder and invoices to that address).{/if}
	<code>--json</code> prints the checklist for agents and scripts. For a registry created before the terms,
	<code>cargo privatecrates terms</code> records an admin’s acceptance.
</p>
<CodeBlock caption="shell" code={setupCommands(org, org)} />
<p>The storage repository is created with your own GitHub login, not ours:</p>
<CodeBlock caption="shell" code={storageRepoCommands(org)} />

<h3 id="cli-init">init</h3>
<p>
	Run in a crate repository or workspace. It adds the registry to <code>.cargo/config.toml</code>, keeping its
	formatting; sets <code>package.repository</code> from the git remote where it is missing (in
	<code>[workspace.package]</code> for workspaces); and writes <code>.github/workflows/publish.yml</code>.
	Running it again changes nothing, and it prints what it changed. <code>--url</code> gives the registry’s full
	URL instead of its name’s default.
</p>
<CodeBlock caption="shell" code={initCommands(org)} />

<h3 id="cli-doctor">doctor</h3>
<p>
	Checks that the credential provider is installed and configured, the registry answers,
	<code>package.repository</code> matches the git remote, and the publish workflow has
	<code>id-token: write</code>, and that <code>publish</code> is restricted to the registry. Once a version is
	published, it checks that it is in the registry’s index. Immutable releases and provenance are checked by
	<a href="/docs/verify">the verifier</a>, which runs on the storage repository.
</p>
<CodeBlock caption="shell" code={doctorCommands()} />

<h2 id="security">Security</h2>
<ul>
	<li>
		<strong>No broad tokens.</strong> Setting up and publishing need no personal access token. If an agent
		asks for one, or offers to create one, say no. The one exception is deploying an app from a platform that
		builds from Git, such as Railway: there the agent may ask whether you want a
		<a href="/docs/deploy#token">fine-grained, read-only token</a>, which you create and store on the platform
		yourself. The agent never sees it.
	</li>
	<li>
		<strong>Administration stays with you.</strong> Creating the storage repository and turning on immutable
		releases use your own <code>gh</code> login, so the PrivateCrates Apps keep their narrow permissions: metadata
		read for the reader App, and contents write on one repository for the storage App.
	</li>
	<li>
		<strong>The agent’s own token is narrow.</strong> <code>cargo privatecrates login</code> gets a reader App
		token that can read repository metadata and nothing else, valid for 8 hours.
		<code>cargo privatecrates logout</code> deletes it from the machine, and you can revoke it at once on GitHub
		under Settings → Applications → Authorized GitHub Apps.
	</li>
	<li>
		<strong>You accept the terms, not the agent.</strong> Accepting binds your organisation, so only you can do
		it, after reading them.
	</li>
	<li>
		<strong>You review every change.</strong> The agent opens pull requests; you merge them. CI publishes with GitHub
		Actions’ OIDC token, so no secret is added to any repository.
	</li>
</ul>
<Callout title="Where the agent stops">
	<p>
		At each step that needs you, the agent gives you a link and waits. Open it, do what it says on GitHub,
		then tell the agent you are done; it confirms the step before going on.
	</p>
	<p>
		The terms are yours to accept. The agent shows you the link to the <a href={TERMS_PATH}>preview terms</a>
		and passes <code>--accept-terms</code> only after you say you accept. It must never accept them for you.
	</p>
</Callout>

<style>
	.who {
		font-weight: 700;
		white-space: nowrap;
	}
	.who.agent {
		font-weight: 400;
		color: var(--ink-soft);
	}
</style>
