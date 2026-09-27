<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { cli, verifyWorkflow } from '$lib/snippets';

	const output = `Verified: nothing to report.`;
</script>

<Seo
	title="Verify the registry"
	description="Run the open-source privatecrates-verify in your own CI to check every version, provenance record and index change PrivateCrates makes."
	path="/docs/verify"
/>

<h1>Verify the registry</h1>
<p class="lede">
	Don’t trust us, verify us. <code>privatecrates-verify</code> is open source and runs in your own CI with a read-only
	token. It checks everything PrivateCrates has written to your storage repository.
</p>

<h2 id="workflow">Run it on every publish</h2>
<p>
	Add this workflow to the storage repository itself. The <a href="/account">account page</a> offers it once
	your registry is live: <strong>Add it on GitHub</strong> opens the file, filled in, for you to commit with
	your own account. From a terminal, <code>{cli('add-verifier acme')}</code> commits it with your own
	<code>gh</code>
	login. We never add it ourselves: our storage App cannot write workflows, and should not maintain what checks
	it.
</p>
<CodeBlock
	caption="crates-store/.github/workflows/privatecrates-verify.yml"
	code={verifyWorkflow('https://acme.privatecrates.dev')}
/>
<p>
	It runs on every push to the storage repository, so after every publish, yank and settings change, and daily
	for changes to releases, which are not pushes. It needs the full git history (<code>fetch-depth: 0</code>)
	and uses the job’s own <code>GITHUB_TOKEN</code> with <code>contents: read</code>. One cache keeps a small
	state file of the versions already verified, so each run checks only what is new; another keeps the
	verifier, pinned to a version, so it is built once. A run takes about a minute of Actions time.
</p>

<h2 id="checks">What it reports</h2>
<ul>
	<li>
		Any change to the index history other than appends by our storage App and <code>yanked</code> flips.
	</li>
	<li>
		Any change by the storage App to an existing owners file or to <code>privatecrates.toml</code>. The App
		only ever creates an owners file at a crate’s first publish; every other change there must come from a
		person.
	</li>
	<li>
		Any version whose release is missing, is not immutable, or whose <code>.crate</code> digest differs from the
		index checksum.
	</li>
	<li>
		Any version whose provenance is missing, has an invalid GitHub signature, names a different crate, version
		or checksum, or names a repository, workflow or environment the owners file did not allow at the time.
	</li>
	<li>Every manual publish, for review, on crates that allow them.</li>
	<li>Crate names that clash with crates.io.</li>
</ul>

<h2 id="results">Reading the results</h2>
<p>A clean run prints one line and exits with status 0:</p>
<CodeBlock caption="output" code={output} hashComments={false} />
<p>
	Otherwise it prints one line per finding, labelled <code>ERROR</code>, <code>warning</code> or
	<code>info</code>, and exits with status 1 if there is any error, which fails the workflow and notifies you
	the way any failed workflow does. Status 2 means it could not run at all. Add <code>--json</code> for machine-readable
	findings.
</p>

<Callout tone="warn" title="Run it promptly">
	<p>
		GitHub rotates the keys that sign OIDC tokens, and old keys eventually disappear. Provenance has to be
		checked while the key is still published, which is why the workflow runs on every publish. A version the
		verifier could not check in time is reported.
	</p>
</Callout>

<h2 id="options">Options</h2>
<div class="table-scroll">
	<table>
		<thead>
			<tr><th scope="col">Option</th><th scope="col">Default</th><th scope="col">Purpose</th></tr>
		</thead>
		<tbody>
			<tr><td><code>--registry</code></td><td>required</td><td>Your registry’s base URL.</td></tr>
			<tr>
				<td><code>--repo</code></td><td><code>GITHUB_REPOSITORY</code></td><td
					>The storage repository, <code>owner/name</code>.</td
				>
			</tr>
			<tr
				><td><code>--token</code></td><td><code>GITHUB_TOKEN</code></td><td
					>A token that can read the storage repository.</td
				></tr
			>
			<tr><td><code>--clone</code></td><td><code>.</code></td><td>The local clone, with full history.</td></tr
			>
			<tr>
				<td><code>--state</code></td><td><code>.privatecrates-verify.json</code></td><td
					>Where to record what has been verified.</td
				>
			</tr>
			<tr><td><code>--json</code></td><td>off</td><td>Print findings as JSON.</td></tr>
		</tbody>
	</table>
</div>
