<script lang="ts">
	import { COMPANY } from '$lib/site';
	import Callout from '$lib/components/Callout.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import { FREE_MEMBER_LIMIT } from '$lib/site';
	import { SECURITY_EMAIL } from '$lib/trust';

	const toc: TocItem[] = [
		['scope', 'Scope'],
		['rules', 'How to test'],
		['safe-harbour', 'Safe harbour'],
		['report', 'What to include'],
		['response', 'What happens next']
	];
</script>

<Seo
	title="Vulnerability disclosure policy"
	description="How to report a security problem in PrivateCrates: what is in scope, what to include, our safe harbour for good-faith research, and how we respond."
	path="/security"
/>

<div class="page">
	<div class="policy">
		<div class="prose intro">
			<h1>Vulnerability disclosure policy</h1>
			<p class="lede">
				Found a security problem in PrivateCrates? Email
				<a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>. We read every report, and we would rather hear
				about a problem twice than not at all.
			</p>

			<Callout title="Draft for review">
				<p>
					The safe-harbour wording below has not been reviewed by a lawyer yet, and the response targets are
					not set. Highlighted items are not decided yet.
				</p>
			</Callout>
		</div>
		<TocLayout {toc}>
			<h2 id="scope">Scope</h2>
			<p>In scope:</p>
			<ul>
				<li>
					The website and account API at <code>privatecrates.dev</code>, and registries at
					<code>*.privatecrates.dev</code>.
				</li>
				<li>
					The registry protocol as we implement it: sign-in, token exchange, permissions, publishing, yanking,
					search and downloads.
				</li>
				<li>
					How our GitHub Apps use their permissions, including anything that lets us, or anyone else, write to
					a storage repository in a way the verifier does not report.
				</li>
				<li>
					The open-source tools: the credential provider, <code>cargo privatecrates</code> and the verifier.
				</li>
			</ul>
			<p>Out of scope:</p>
			<ul>
				<li>
					GitHub, Stripe, Railway and Cloudflare themselves. Report those to them; tell us too if it affects
					PrivateCrates.
				</li>
				<li>Denial of service, load testing, spam and social engineering of our staff or customers.</li>
				<li>
					Reports from automated scanners without a demonstrated impact, such as a missing header that enables
					no attack.
				</li>
			</ul>

			<h2 id="rules">How to test</h2>
			<ul>
				<li>
					Test against a GitHub organisation and registry you own. Organisations with up to {FREE_MEMBER_LIMIT}
					members are free.
				</li>
				<li>
					Do not access, change or delete other people’s data. If you reach any by accident, stop, and tell us
					what you saw.
				</li>
				<li>Do not degrade the service for others.</li>
				<li>
					Give us 90 days to fix a problem before you publish it, or less once a fix has shipped. For a
					complex problem we may ask for longer, and agree it with you.
				</li>
			</ul>

			<h2 id="safe-harbour">Safe harbour</h2>
			<p>
				If you follow this policy in good faith, we will consider your research authorised, will not pursue
				legal action against you over it, and will not ask anyone else to. If a third party takes action
				against you for research that followed this policy, we will make it known that your work was
				authorised.
			</p>
			<p>
				This commitment is given by {COMPANY.name} (company number {COMPANY.number}), which runs
				PrivateCrates. It will be reviewed by a lawyer before general availability.
			</p>

			<h2 id="report">What to include</h2>
			<ul>
				<li>What the problem is, and what an attacker could do with it.</li>
				<li>
					Steps to reproduce: the requests, commands or code, and the registry, organisation or crate you
					used.
				</li>
				<li>
					Whether any data other than your own was involved. Please do not include tokens that still work;
					revoke them first.
				</li>
				<li>How you would like to be credited, if at all.</li>
			</ul>
			<p>Encrypted email is not offered yet; it is on the roadmap for general availability.</p>

			<h2 id="response">What happens next</h2>
			<ul>
				<li><strong>Acknowledgement:</strong> within 3 business days.</li>
				<li>
					<strong>Triage and first assessment:</strong> within 5 business days, on a best-effort basis during the
					preview.
				</li>
				<li>
					<strong>Fix:</strong>
					critical problems (tokens, crates or another organisation’s data exposed, or publishing without permission)
					within 7 days; others as soon as practical, on a best-effort basis during the preview. We tell you when
					the fix is deployed, and post on the status page if customers need to act.
				</li>
				<li>
					<strong>Credit:</strong> with your permission, we thank you by name in the release notes of the fix. We
					do not run a paid bug bounty.
				</li>
			</ul>
			<p>
				The same contact is published in <a href="/.well-known/security.txt" data-sveltekit-reload
					>/.well-known/security.txt</a
				>. For how the service is secured, see the <a href="/trust">trust centre</a> and the
				<a href="/docs/security">security model</a>.
			</p>
		</TocLayout>
	</div>
</div>

<style>
	.policy {
		padding-top: 2.5rem;
	}
	.intro {
		margin-bottom: 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
	}
</style>
