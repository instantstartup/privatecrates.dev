<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import Lighthouse from '$lib/illustrations/Lighthouse.svelte';

	const threats = [
		['A developer’s token is stolen', 'The thief can list repository metadata. Nothing else.'],
		[
			'A CI registry token is stolen',
			'The thief can read your crates for under an hour. We can revoke every outstanding token at once by rotating the signing key.'
		],
		[
			'Our storage App key is stolen, or PrivateCrates is compromised',
			'Published bytes cannot be changed: releases are immutable. A malicious new version has no valid provenance, and a changed owners file or rewritten index line is not an App-authored append; the verifier reports all three. On a crate that allows manual publishing, a forged version looks like a manual publish, which is why manual publishing is off by default and the verifier lists every one for review. Deleting releases breaks downloads, and is reported.'
		],
		['GitHub itself is compromised', 'Out of scope: GitHub is the root of trust by design.']
	];
</script>

<Seo
	title="Security model"
	description="What PrivateCrates sees, stores and can do; the threats it is designed to survive; and how immutable releases and provenance make our writes verifiable."
	path="/docs/security"
/>

<div class="title-row">
	<h1>Security model</h1>
	<Lighthouse class="title-art" beams={false} />
</div>
<p class="lede">
	GitHub is the root of trust. PrivateCrates is designed so that stealing our keys, or taking over the whole
	service, does not let anyone change a published crate without your verifier noticing.
</p>

<h2 id="boundary">The trust boundary</h2>
<ul>
	<li><strong>What we see:</strong> callers’ tokens, and crate bytes in transit while they are published.</li>
	<li>
		<strong>What we store durably:</strong> no registry data and no code. Caches only, keyed by hashes of
		tokens, never tokens. The durable records are each organisation’s acceptance of the
		<a href="/legal/terms">terms</a> (who accepted, for which organisation, when) and requests to join the private
		preview.
	</li>
	<li>
		<strong>What we can do:</strong> write to each organisation’s storage repository, through the storage App.
	</li>
	<li>
		<strong>What we depend on:</strong> GitHub for storage, identity and permissions, and Railway and
		Cloudflare to run the service. When one of them fails, PrivateCrates may too; we are not responsible for
		them (<a href="/legal/terms#dependencies">terms, section 9</a>), and the
		<a href="/trust#dependencies">trust centre</a> explains how the status page tells their problems from ours.
	</li>
</ul>

<h2 id="threats">Threats and outcomes</h2>
<div class="table-scroll">
	<table>
		<thead>
			<tr><th scope="col">Threat</th><th scope="col">Outcome</th></tr>
		</thead>
		<tbody>
			{#each threats as [threat, outcome] (threat)}
				<tr><th scope="row">{threat}</th><td>{outcome}</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<h2 id="permissions">Permissions are GitHub’s</h2>
<p>
	A caller may do what GitHub allows them to do on the repository that owns a crate: read it to use the crate,
	push to it to publish or yank. Two GitHub Apps keep our own access narrow:
</p>
<div class="table-scroll">
	<table>
		<thead>
			<tr><th scope="col">App</th><th scope="col">Installed on</th><th scope="col">Permissions</th></tr>
		</thead>
		<tbody>
			<tr
				><th scope="row">Reader</th><td>Repositories that own crates</td><td
					>Metadata read; organisation members read</td
				></tr
			>
			<tr><th scope="row">Storage</th><td>The storage repository only</td><td>Contents read and write</td></tr
			>
		</tbody>
	</table>
</div>
<p>
	Neither App has repository administration, so neither can turn immutable releases off. Removing someone from
	your organisation or a team reaches us by webhook and cuts off their access within seconds. A crate someone
	cannot read is a 404, never a 403, so its existence is not revealed.
</p>

<h2 id="controls">Controls</h2>
<ul>
	<li><strong>Tokens</strong> are never logged, never stored and never sent anywhere but GitHub’s API.</li>
	<li>
		<strong>Narrow tokens by default:</strong> developers hold reader App tokens (metadata read); CI holds one-hour
		registry tokens with no GitHub access at all.
	</li>
	<li>
		<strong>Secrets:</strong> both Apps’ private keys and the key that signs registry tokens are held as
		encrypted variables at our hosting provider, never in the repository. Moving them into a cloud key
		management service, so the keys never exist in our environment at all, is on the
		<a href="/trust#certifications">roadmap</a>.
	</li>
	<li><strong>Transport:</strong> HTTPS only, with HSTS.</li>
	<li>
		<strong>Limits:</strong> capped request sizes, a publish rate limit per token, and timeouts on every call to
		GitHub.
	</li>
</ul>

<h2 id="integrity">Integrity and provenance</h2>
<ul>
	<li>
		<strong>Immutable releases:</strong> once published, a release’s tag and files can never be modified or deleted,
		whoever holds our keys.
	</li>
	<li>
		<strong>Checksums at publish:</strong> we compute the SHA-256 ourselves and compare it with the digest GitHub
		computes for the uploaded file before the release is published.
	</li>
	<li>
		<strong>Provenance:</strong> a CI publish stores the bound OIDC token with the release. It is signed by GitHub,
		not by us, and names the repository, workflow, crate, version and checksum.
	</li>
	<li>
		<strong>Append-only index:</strong> we only ever append lines or flip <code>yanked</code>, and refuse any
		other rewrite.
	</li>
	<li>
		<strong>Every write is a commit</strong> in your repository, made by the storage App so GitHub marks it verified,
		with the publisher’s GitHub login in the message.
	</li>
	<li>
		<strong>The verifier</strong> checks all of the above in your CI. <a href="/docs/verify">Set it up</a>.
	</li>
</ul>

<h2 id="trust">Trust centre</h2>
<p>
	Where the service runs, our subprocessors, certifications, incident response and how to report a
	vulnerability are on the <a href="/trust">trust centre</a>.
</p>

<h2 id="source">Source code</h2>
<p>
	The credential provider and the verifier are open source under MIT or Apache-2.0. The hosted service is
	source-available under the Business Source License 1.1: anyone may read and audit it, and self-host it for
	their own organisation. Each version converts to Apache-2.0 four years after release.
</p>

<style>
	.title-row {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 1rem;
	}
	.title-row :global(.title-art) {
		width: 5.5rem;
		height: auto;
		flex: none;
	}
</style>
