<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { dockerfileWithArg, dockerfileWithSecret, registryEnv } from '$lib/snippets';

	const env = registryEnv('acme');
</script>

<Seo
	title="Deploying apps"
	description="Deploy an app that depends on private crates to Railway, Render, Fly.io, Cloud Run and others: from GitHub Actions with no secrets, or with a narrow, read-only GitHub token for platforms that build from Git."
	path="/docs/deploy"
/>

<h1>Deploying apps</h1>
<p class="lede">
	An app that depends on private crates needs them wherever it is built. Where that is decides what you need:
	nothing at all when the build runs in GitHub Actions, or a narrow, read-only GitHub token when your platform
	builds the image itself.
</p>

<h2 id="which">Which way your platform deploys</h2>
<div class="table-scroll">
	<table>
		<thead>
			<tr>
				<th scope="col">Platform</th>
				<th scope="col">Its usual way to deploy</th>
				<th scope="col">What the build needs</th>
			</tr>
		</thead>
		<tbody>
			<tr>
				<td>Fly.io, Google Cloud Run, AWS, Azure</td>
				<td>From GitHub Actions</td>
				<td>Nothing: <a href="#actions">vendor the crates in the job</a></td>
			</tr>
			<tr>
				<td>Railway, Render, Heroku, DigitalOcean App Platform</td>
				<td>The platform builds from your Git repository</td>
				<td><a href="#platform-builds">A fine-grained GitHub token</a>, as a build secret</td>
			</tr>
		</tbody>
	</table>
</div>

<h2 id="actions">Deploying from GitHub Actions</h2>
<p>
	In Actions the credential provider needs no secret: the job’s OIDC token is the credential. Let the job
	fetch every dependency with <code>cargo vendor</code>, then build the image offline or hand the directory to
	your platform’s deploy command (<code>fly deploy</code>, <code>gcloud run deploy --source .</code>). Nothing
	that reaches the image or the platform can read the registry.
	<a href="/docs/ci#elsewhere">CI without secrets</a> has the workflow and the Dockerfile.
</p>

<h2 id="platform-builds">Platforms that build from Git</h2>
<p>
	Railway, Render and similar platforms build your Dockerfile themselves when you push, on their own machines,
	which have no GitHub identity for us to check. Give the build a GitHub token instead: a fine-grained token
	that can see only the repositories your app’s crates come from, and nothing in them but their metadata.
	GitHub creates, expires and revokes it; PrivateCrates never issues or stores it, and only asks GitHub what
	it may read, as it does for every token.
</p>

<h3 id="token">The token</h3>
<ol>
	<li>
		Sign in to GitHub as a service account: a GitHub user for machines, which many organisations already have.
		It needs read access to the repositories your app’s crates are published from. (A person’s own account
		works too, but the token stops working when they leave.)
	</li>
	<li>
		Open <em>Settings → Developer settings → Fine-grained tokens → Generate new token</em>. Choose:
		<ul>
			<li>
				<strong>Resource owner:</strong> your organisation. If it is not in the list, an organisation owner
				first allows fine-grained tokens, under <em>Organisation settings → Personal access tokens</em>;
			</li>
			<li>
				<strong>Expiration:</strong> a date, and a reminder to rotate it (your organisation may set a maximum);
			</li>
			<li>
				<strong>Repository access:</strong> <em>Only select repositories</em>, choosing the repositories whose
				crates the app depends on. The token can download exactly those crates; every other crate looks as if
				it does not exist.
			</li>
			<li>
				<strong>Permissions:</strong> under <em>Repository permissions</em>, add <em>Metadata</em> and set it
				to <em>Read-only</em>, and nothing else. GitHub does not always add it for you, and without it the
				token cannot see the repositories at all. It is all the registry needs: the token cannot read code or
				change anything.
			</li>
		</ul>
	</li>
	<li>
		If your organisation requires approval for fine-grained tokens, an owner approves it under
		<em>Organisation settings → Personal access tokens</em>.
	</li>
	<li>Store it on your platform as described below, and nowhere else.</li>
</ol>
<Callout title="Why not a classic token?">
	<p>
		A classic personal access token reaches every repository the account can, with broad scopes. The
		fine-grained token above reaches only the repositories you pick, read-only, and your organisation’s owners
		can see and revoke it.
	</p>
</Callout>

<p>
	If the build then finds no crates (“not found” for each, or “no access” for the registry), check the token
	on GitHub: that its resource owner is the organisation, that an owner has approved it if your organisation
	requires that, that <em>Metadata: Read-only</em> is listed, and that the crate’s repository is selected. The registry
	answers exactly as GitHub lets the token see.
</p>

<h3 id="render">Render</h3>
<p>
	Add the token as a <em>Secret File</em> named <code>privatecrates-token</code>. Render mounts secret files
	into Docker builds as build secrets, so the token is a file only while <code>cargo build</code> runs and
	never part of an image layer. <code>cargo:token</code> makes Cargo send it as it is, in place of the
	credential provider your
	<code>.cargo/config.toml</code> names for developers (<code>{env}_…</code> is for a registry named
	<code>acme</code>).
</p>
<CodeBlock caption="Dockerfile" code={dockerfileWithSecret('acme')} />

<h3 id="railway">Railway</h3>
<p>
	Add the token as a <em>sealed</em> service variable named <code>PRIVATECRATES_TOKEN</code>: Railway passes
	it to the build, but never shows it again. Railway gives variables to Docker builds only as build arguments,
	so declare it in the build stage alone. The image that runs is the second stage, which copies just the
	binary, so it contains no trace of the token.
</p>
<CodeBlock caption="Dockerfile" code={dockerfileWithArg('acme')} />

<h3 id="others">Other platforms</h3>
<p>
	The same two lines work anywhere: set <code>{env}_CREDENTIAL_PROVIDER=cargo:token</code> and
	<code>{env}_TOKEN</code> for the <code>cargo build</code> step only. Prefer a build secret where the platform
	has one; otherwise a build argument declared in a build stage that the final image does not inherit.
</p>

<h2 id="rotate">Rotating and revoking</h2>
<p>
	Everything happens on GitHub. To rotate, regenerate the token and update the platform’s secret. To revoke
	it, delete it from the service account, or, as an organisation owner, under <em
		>Organisation settings → Personal access tokens</em
	>. Our permission cache forgets it within five minutes. To let the app use crates from another repository,
	edit the token’s repository access; no new token is needed.
</p>
