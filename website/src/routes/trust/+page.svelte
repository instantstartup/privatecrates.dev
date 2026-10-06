<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { GA_TARGET, TERMS_PATH } from '$lib/site';
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import Lighthouse from '$lib/illustrations/Lighthouse.svelte';
	import { SECURITY_EMAIL, STATUS_URL, subprocessors } from '$lib/trust';

	/** docs/preview.md §4, in order: [item, why]. */
	const roadmap: [string, string][] = [
		['Form a company; publish its name, number and address', 'The operator for terms, DPA and billing'],
		[
			'Legal review of the terms, privacy notice and DPA (governed by the law of England and Wales)',
			'Terms fit for paying customers'
		],
		[
			'International transfer mechanism (EU SCCs and the UK addendum)',
			'EU customers’ data processed in the US'
		],
		[
			'Turn on billing (Stripe live mode), with at least 30 days’ notice to existing organisations',
			'Revenue'
		],
		[
			'Incident commitments: time to first status update, breach notification deadline (72 hours)',
			'Enterprise reviews ask'
		],
		[
			'Security response targets by severity; a PGP key or other encrypted channel',
			'Disclosure policy completeness'
		],
		['App keys and the token-signing key in a KMS', 'Least exposure of the most sensitive keys'],
		['Independent penetration test, with a summary on the trust centre', 'Evidence for reviewers'],
		['SOC 2 Type I, then Type II', 'The usual enterprise gate']
	];

	const toc: TocItem[] = [
		['what-we-hold', 'What we hold'],
		['where-it-runs', 'Where it runs'],
		['subprocessors', 'Subprocessors'],
		['security', 'Security controls'],
		['certifications', 'Certifications'],
		['roadmap', 'Roadmap'],
		['incidents', 'Incident response'],
		['disclosure', 'Vulnerability disclosure'],
		['documents', 'Documents']
	];
</script>

<Seo
	title="Trust centre"
	description="What PrivateCrates holds (records of who accepted the terms), where it runs, who else touches your data, our security controls, certifications, incident response and vulnerability disclosure."
	path="/trust"
/>

<div class="page">
	<header class="hero">
		<div>
			<h1>Trust centre</h1>
			<p class="lede">
				What we hold, who else touches it, and how you can check us. The short answer: your crates live in
				your own GitHub organisation. We store only a record of who accepted our terms, and requests to join
				the preview.
			</p>
		</div>
		<Lighthouse class="hero-art" />
	</header>

	<!-- The one thing a security reviewer asks first, answered before anything else. -->
	<section class="manifest panel" aria-labelledby="manifest-title">
		<h2 id="manifest-title">Held by PrivateCrates</h2>
		<dl>
			<div class="row nothing">
				<dt>Stored durably</dt>
				<dd>
					<strong>Only terms acceptances</strong><br />Which organisation, which GitHub user, which terms
					version, when. No registry data, no code, no tokens.
				</dd>
			</div>
			<div class="row">
				<dt>In transit</dt>
				<dd>Callers’ GitHub and registry tokens, and crate files while they are being published.</dd>
			</div>
			<div class="row">
				<dt>In memory</dt>
				<dd>
					Caches keyed by a hash of the token, never the token: who can read what, index files, crate metadata
					for search, member counts.
				</dd>
			</div>
			<div class="row">
				<dt>In logs</dt>
				<dd>
					GitHub logins, organisation and registry names, crate names and versions, errors. Never tokens.
				</dd>
			</div>
		</dl>
	</section>

	<div class="body">
		<TocLayout {toc}>
			<h2 id="what-we-hold">What we hold</h2>
			<p>
				PrivateCrates runs the Cargo registry protocol in front of a repository your organisation owns. The
				index and every crate file are stored there, by GitHub, as commits and immutable releases. The service
				itself keeps records of terms acceptances, fingerprints of the provenance it has verified, and the
				last of the invitation requests from when the preview was by invitation; everything else it knows is
				on GitHub, or can be rebuilt from it.
			</p>
			<h3>Stored: terms acceptances</h3>
			<p>
				When an admin accepts the <a href={TERMS_PATH}>terms</a> for an organisation, we record the organisation’s
				GitHub ID and login, the admin’s GitHub ID and login, the terms version, the time, whether they accepted
				on the website or with the CLI, and the exact statement they accepted. The record holds no secrets and is
				never shared.
			</p>
			<p>
				It is kept in a Postgres database in the same Railway project and region as the service (US East),
				with Railway’s backups, for as long as the organisation uses PrivateCrates and then for 6 years, as
				evidence of the agreement.
			</p>
			<h3>Stored: fingerprints of verified provenance</h3>
			<p>
				The compliance dashboard checks each version’s signed provenance once, and remembers that it passed as
				a SHA-256 fingerprint of the version, its checksum and its files, in the same database. A fingerprint
				names no crate and cannot be turned back into one; it only spares the dashboard downloading the same
				file again.
			</p>
			<h3>Stored: invitation requests</h3>
			<p>
				Until 6 October 2026 the preview was by invitation, and people who asked to join gave their GitHub ID
				and login, the organisation they named, an email address and a note, kept in the same database. We use
				them only to tell those people the preview is open, then delete them; at the latest, each is deleted
				12 months after it was made. We no longer collect them.
			</p>
			<h3>Passes through, not kept</h3>
			<ul>
				<li>
					<strong>Tokens.</strong> Developers’ GitHub tokens and CI’s one-hour registry tokens arrive with each
					request. We use them only to ask GitHub what the caller may access, and never store, log or forward them
					anywhere else.
				</li>
				<li>
					<strong>Crate files</strong> during <code>cargo publish</code>: we check the file, compute its
					SHA-256 and upload it to your storage repository.
				</li>
				<li>
					<strong>Your website sign-in</strong> lives in an encrypted cookie in your browser for 8 hours. Nothing
					is kept on our side.
				</li>
			</ul>
			<h3>Cached in memory</h3>
			<p>
				Caches make the registry fast and are lost on every restart. They are keyed by a hash of the token,
				never the token itself: which repositories a token can read (5 minutes, and dropped by webhook when
				membership changes), index files, crate metadata for <code>cargo search</code>, and each
				organisation’s member count (up to 24 hours, a number only).
			</p>
			<h3>Logged</h3>
			<p>
				The service writes structured logs: GitHub logins of people who set up or publish, organisation and
				registry names, crate names and versions, the workflow that published them, and error codes and
				messages. Tokens and crate contents are never logged.
			</p>
			<p>Logs are kept for 30 days, Railway’s retention period, then deleted.</p>

			<h2 id="where-it-runs">Where it runs</h2>
			<div class="table-scroll">
				<table>
					<thead>
						<tr><th scope="col">Part</th><th scope="col">Runs on</th></tr>
					</thead>
					<tbody>
						<tr>
							<th scope="row">The service and this website</th>
							<td>
								Railway, one replica per environment, in the US East (Virginia) region, close to GitHub’s API.
								Railway terminates TLS for our domains.
							</td>
						</tr>
						<tr>
							<th scope="row">DNS</th>
							<td>
								Cloudflare, DNS only. Records are not proxied, so requests go straight to Railway and
								Cloudflare never sees their contents.
							</td>
						</tr>
						<tr>
							<th scope="row">Your crates and index</th>
							<td>
								GitHub, in a private repository in your own organisation, under your agreement with GitHub.
								Deleting that repository deletes your registry’s data.
							</td>
						</tr>
						<tr>
							<th scope="row">Terms acceptances</th>
							<td>
								A Postgres database at Railway, in the same project and region as the service, with Railway’s
								backups.
							</td>
						</tr>
						<tr>
							<th scope="row">Billing</th>
							<td>
								Stripe, from general availability. Billing is off during the preview, so Stripe is listed but
								not yet used. Card details will be entered on Stripe’s pages and never reach us.
							</td>
						</tr>
						<tr>
							<th scope="row">Status page</th>
							<td>
								A Cloudflare Worker at <a href={STATUS_URL}>status.privatecrates.dev</a>, deliberately
								separate from Railway so it stays up when we are down.
							</td>
						</tr>
					</tbody>
				</table>
			</div>

			<h2 id="subprocessors">Subprocessors</h2>
			<p>
				The companies that process customer data on our behalf. We announce a new subprocessor here at least
				30 days before it starts, and, once billing is on, email each organisation’s billing address too. If
				you object and we cannot accommodate it, you may cancel and get a prorated refund of anything prepaid.
				An urgent replacement, after a failure or a breach, may happen at once, with notice as soon as we can
				give it.
			</p>
			<div class="table-scroll">
				<table class="subprocessors table-cards">
					<thead>
						<tr>
							<th scope="col">Subprocessor</th>
							<th scope="col">Purpose</th>
							<th scope="col">Data</th>
							<th scope="col">Location</th>
						</tr>
					</thead>
					<tbody>
						{#each subprocessors as s (s.name)}
							<tr>
								<th scope="row"><a href={s.url}>{s.name}</a></th>
								<td data-label="Purpose">{s.purpose}</td>
								<td data-label="Data">{s.data}</td>
								<td data-label="Location">{s.location}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			<h3 id="dependencies">When a service we depend on fails</h3>
			<p>
				PrivateCrates runs on these services and cannot work without them, GitHub above all: it stores every
				registry, signs everyone in and decides who may read and publish. When one of them is down or
				degraded, parts of PrivateCrates, or all of it, may stop working, and there may be nothing we can do
				until it recovers. We do not control them and are not responsible for them; the
				<a href="{TERMS_PATH}#dependencies">terms</a> say so formally. The
				<a href={STATUS_URL}>status page</a>
				checks their public status every minute and says when a problem is theirs rather than ours.
			</p>

			<h2 id="security">Security controls</h2>
			<p>
				GitHub is the root of trust. The service is designed so that stealing our keys, or taking it over,
				does not let anyone change a published crate without your own verifier noticing. The
				<a href="/docs/security">security model</a> has the threat table in full.
			</p>
			<h3>Access</h3>
			<ul>
				<li>
					<strong>Permissions are GitHub’s.</strong> A caller may read a crate if they can read the repository that
					owns it, and publish if they can push to it. Removing someone on GitHub cuts off their access by webhook
					within seconds.
				</li>
				<li>
					<strong>Two least-privilege GitHub Apps.</strong> The reader App has metadata read and members read. The
					storage App can write only to your storage repository. Neither has repository administration, so neither
					can turn immutable releases off.
				</li>
				<li>
					<strong>Narrow tokens.</strong> Developers hold reader App tokens that can list metadata and nothing else.
					CI holds one-hour registry tokens with no GitHub access at all.
				</li>
			</ul>
			<h3>Integrity</h3>
			<ul>
				<li>
					<strong>Immutable releases:</strong> a published crate file can never be changed or deleted, whoever holds
					our keys.
				</li>
				<li>
					<strong>Checksums at publish:</strong> we compare our SHA-256 with the digest GitHub computes before the
					release goes live.
				</li>
				<li>
					<strong>Provenance:</strong> each CI publish stores a token signed by GitHub, not by us, naming the repository,
					workflow, crate, version and checksum.
				</li>
				<li>
					<strong>Append-only index, and every write a verified commit</strong> in your repository, with the publisher
					in the message.
				</li>
				<li>
					<strong>The open-source verifier</strong> checks all of this from your own CI.
					<a href="/docs/verify">Set up the verifier</a>.
				</li>
			</ul>
			<h3>Transport, sessions and secrets</h3>
			<ul>
				<li>
					<strong>HTTPS only</strong>, with HSTS for a year on every subdomain. The website sends a strict
					Content-Security-Policy with no inline scripts, and refuses to be framed.
				</li>
				<li>
					<strong>Session cookie</strong> <code>pc_session</code>: encrypted with AES-256-GCM,
					<code>HttpOnly</code>,
					<code>Secure</code>, <code>SameSite=Lax</code>, 8 hours.
				</li>
				<li>
					<strong>Secrets</strong> (GitHub App keys, signing and session keys, Stripe keys) are generated per environment
					and held as encrypted Railway variables, never in the repository. Each has a rotation procedure. Moving
					the App keys and the token signing key into a cloud key management service is on the roadmap below; today
					they are not in one.
				</li>
				<li>
					<strong>Limits:</strong> capped request sizes, a publish rate limit per token, and timeouts on every call
					to GitHub.
				</li>
				<li>
					<strong>Changes:</strong> production deploys only from a protected branch, after CI passes on the exact
					commit.
				</li>
			</ul>
			<h3>Source code</h3>
			<p>
				The credential provider, the set-up CLI and the verifier are open source under MIT or Apache-2.0. The
				hosted service is source-available under the Business Source License 1.1, so you can audit what runs.
			</p>

			<h2 id="certifications">Certifications</h2>
			<p>
				<strong>None yet.</strong> PrivateCrates has no SOC 2 report, no ISO 27001 certificate and has not had an
				independent penetration test. We would rather say so than imply otherwise. Until then, the design is the
				assurance: your data stays on GitHub, whose own reports cover it, and the verifier lets you check our writes
				yourself.
			</p>
			<p>
				The penetration test and SOC 2 are on the <a href="#roadmap">roadmap to general availability</a>,
				below.
			</p>
			<p>
				GitHub’s and Stripe’s own reports cover the data they hold: see the
				<a href="https://github.com/security">GitHub Security</a> and
				<a href="https://stripe.com/docs/security">Stripe security</a> pages.
			</p>

			<h2 id="roadmap">Roadmap to general availability: {GA_TARGET}</h2>
			<p>
				PrivateCrates is in <a href={TERMS_PATH}>preview</a>: free, run by one person, and provided as is.
				This is what has to happen before general availability, in order. We will do it if there is enough
				interest.
			</p>
			<div class="table-scroll">
				<table class="roadmap table-cards">
					<thead>
						<tr>
							<th scope="col"><span class="visually-hidden">Order</span></th>
							<th scope="col">Item</th>
							<th scope="col">Why</th>
						</tr>
					</thead>
					<tbody>
						{#each roadmap as [item, why], i (item)}
							<tr>
								<td class="num">{i + 1}</td>
								<th scope="row">{item}</th>
								<td data-label="Why">{why}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<h2 id="incidents">Incident response</h2>
			<ul>
				<li>
					<strong>Detection:</strong> the status page probes the website, the account API and a registry from outside
					Railway every minute, and watches GitHub’s and Stripe’s own status.
				</li>
				<li>
					<strong>Communication:</strong> incidents are posted on
					<a href={STATUS_URL}>status.privatecrates.dev</a>, with an Atom feed to subscribe to. When GitHub is
					the cause, the page says so and links GitHub’s incident. For a security incident affecting your
					organisation, we will also email its billing address once billing is on; during the preview there is
					none, so the status page is where we say it.
				</li>
				<li>
					<strong>Target times:</strong>
					none are committed during the preview. Time to first status update and a 72-hour breach notification deadline
					are on the <a href="#roadmap">roadmap</a>.
				</li>
				<li>
					<strong>Limiting the damage:</strong> the only durable data is the terms acceptance record, which holds
					no secrets. Your verifier reports any write we could not have made honestly. Rotating the token signing
					key revokes every outstanding CI token at once.
				</li>
			</ul>

			<h2 id="disclosure">Vulnerability disclosure</h2>
			<p>
				Report security problems to <a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>. Our
				<a href="/security">disclosure policy</a> covers scope, safe harbour and what to include. The same
				contact is published in <a href="/.well-known/security.txt" data-sveltekit-reload>security.txt</a> (RFC
				9116).
			</p>

			<h2 id="documents">Documents</h2>
			<ul class="documents">
				<li>
					<a href="/legal/dpa">Data processing agreement</a>: a template, in draft for legal review, not yet
					in force during the preview.
				</li>
				<li><a href="#subprocessors">Subprocessor list</a>, above.</li>
				<li><a href="/security">Vulnerability disclosure policy</a>.</li>
				<li><a href="/docs/security">Security model</a>, with the threat table.</li>
				<li><a href={TERMS_PATH}>Preview terms</a>, in force during the preview.</li>
				<li>
					<a href="/legal/privacy">Privacy notice</a>, in force. The data processing agreement above: a draft,
					not yet in force during the preview.
				</li>
				<li>
					Licences: MIT or Apache-2.0 for the client tools and the verifier; Business Source License 1.1 for
					the service, converting to Apache-2.0 four years after each release.
				</li>
			</ul>
			<p>
				Need something else for a security review, such as a questionnaire?
				<a href="mailto:{SECURITY_EMAIL}">Email {SECURITY_EMAIL}</a>.
			</p>
		</TocLayout>
	</div>
</div>

<style>
	.hero {
		display: grid;
		gap: 1.5rem;
		align-items: end;
		padding-top: 3rem;
	}
	@media (min-width: 44rem) {
		.hero {
			grid-template-columns: minmax(0, 1fr) 11rem;
		}
	}
	h1 {
		font-size: var(--text-display);
		margin-bottom: 1rem;
	}
	.hero :global(.hero-art) {
		width: 8rem;
		height: auto;
		justify-self: center;
	}
	@media (min-width: 44rem) {
		.hero :global(.hero-art) {
			width: 11rem;
		}
	}

	/* The manifest: a ledger of what we hold, with the one durable record stamped on the first line. */
	.manifest {
		margin-top: 2rem;
		padding: 1.5rem 1.25rem;
		border: 2px solid var(--line);
	}
	@media (min-width: 40rem) {
		.manifest {
			padding: 1.75rem 2rem;
		}
	}
	.manifest h2 {
		font-size: var(--text-xl);
		margin-bottom: 0.75rem;
	}
	.manifest dl {
		display: grid;
	}
	.row {
		display: grid;
		gap: 0.5rem 1.5rem;
		padding-block: 0.75rem;
		border-top: 1px dashed var(--rule);
	}
	@media (min-width: 40rem) {
		.row {
			grid-template-columns: 10rem minmax(0, 1fr);
			align-items: baseline;
		}
	}
	.row dt {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-lg);
		line-height: 1.2;
	}
	.nothing {
		border-top: 0;
	}
	.nothing strong {
		display: inline-block;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		line-height: 1;
		padding: 0.15rem 0.45rem 0.2rem;
		margin-bottom: 0.25rem;
		margin-right: 0.35rem;
		border: 3px solid var(--ok);
		border-radius: 4px;
		color: var(--ok);
		transform: rotate(-2deg);
	}

	.body {
		margin-top: 3rem;
	}
	.subprocessors th[scope='row'] {
		white-space: nowrap;
	}
	/* The order is the plan: a large numeral per step, like the landing page's manifest. */
	.roadmap .num {
		width: 2.5rem;
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		line-height: 1.1;
	}
	.roadmap th[scope='row'] {
		font-size: var(--text-base);
		font-weight: 700;
	}
	@media (max-width: 40rem) {
		.roadmap tr {
			display: grid;
			grid-template-columns: 2.25rem minmax(0, 1fr);
		}
		.roadmap .num {
			grid-row: span 2;
		}
	}
</style>
