<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import DraftBanner from '$lib/components/DraftBanner.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import Todo from '$lib/components/Todo.svelte';
	import { SECURITY_EMAIL, subprocessors } from '$lib/trust';

	const toc: TocItem[] = [
		['parties', '1. Parties'],
		['processing', '2. Processing details'],
		['obligations', '3. Processor obligations'],
		['subprocessors', '4. Subprocessors'],
		['security', '5. Security measures'],
		['breach', '6. Personal data breach'],
		['deletion', '7. Deletion and return'],
		['transfers', '8. International transfers'],
		['general', '9. General']
	];
</script>

<Seo
	title="Data processing agreement (draft)"
	description="Draft template of the PrivateCrates data processing agreement, for legal review: parties, processing details, subprocessors, security measures, breach notification and deletion."
	path="/legal/dpa"
/>

<div class="page">
	<div class="legal">
		<div class="prose intro">
			<h1>Data processing agreement</h1>
			<DraftBanner />
			<Callout title="A template for legal review">
				<p>
					This sets out the structure and the facts of how PrivateCrates processes data. The legal wording,
					the company details and every highlighted gap need review by a lawyer before anyone signs it. The
					facts it relies on are on the <a href="/trust">trust centre</a>.
				</p>
			</Callout>
		</div>
		<TocLayout {toc}>
			<h2 id="parties">1. Parties</h2>
			<ul>
				<li>
					<strong>The customer</strong>, the controller: the organisation that uses PrivateCrates for its
					GitHub organisation, as named in the order or account.
				</li>
				<li>
					<strong>The processor:</strong>
					<Todo>legal entity name, company number and registered address</Todo>, trading as PrivateCrates.
				</li>
			</ul>
			<p>
				This agreement forms part of the <a href="/legal/terms">terms of service</a> between them.
			</p>

			<h2 id="processing">2. Processing details</h2>
			<div class="table-scroll">
				<table>
					<tbody>
						<tr>
							<th scope="row">Subject matter</th>
							<td>Running a private Cargo registry for the customer’s GitHub organisation.</td>
						</tr>
						<tr>
							<th scope="row">Duration</th>
							<td>For as long as the customer uses the service, and then until deletion under section 7.</td>
						</tr>
						<tr>
							<th scope="row">Nature and purpose</th>
							<td>
								Authenticating developers and CI through GitHub; checking their access; publishing crate files
								to the customer’s own storage repository; serving the index and downloads; billing.
							</td>
						</tr>
						<tr>
							<th scope="row">Data subjects</th>
							<td>
								Members of the customer’s GitHub organisation and outside collaborators who use the registry;
								the customer’s billing contact.
							</td>
						</tr>
						<tr>
							<th scope="row">Personal data</th>
							<td>
								GitHub logins, names and avatars (shown at sign-in, not stored); organisation membership and
								roles; the GitHub login of each publisher, written into the customer’s repository; the billing
								email address, held by Stripe; GitHub tokens, in transit only. <Todo
									>confirm whether client IP addresses appear in Railway’s logs</Todo
								>
							</td>
						</tr>
						<tr>
							<th scope="row">Special categories</th>
							<td>None.</td>
						</tr>
					</tbody>
				</table>
			</div>

			<h2 id="obligations">3. Processor obligations</h2>
			<ul>
				<li>
					Process personal data only on the customer’s documented instructions, including this agreement.
				</li>
				<li>Ensure that people authorised to process the data are bound by confidentiality.</li>
				<li>Apply the security measures in section 5.</li>
				<li>
					Help the customer respond to data subjects’ requests and meet its obligations on security, breach
					notification and impact assessments, taking into account that the service stores no personal data
					durably.
				</li>
				<li>
					Make available the information needed to show compliance, and allow audits <Todo
						>audit terms: notice, frequency, cost, and reliance on third-party reports</Todo
					>.
				</li>
			</ul>

			<h2 id="subprocessors">4. Subprocessors</h2>
			<p>
				The customer authorises the subprocessors below. The processor will tell the customer before adding or
				replacing one, giving the customer the chance to object <Todo
					>notice period and how notice is given</Todo
				>.
			</p>
			<div class="table-scroll">
				<table class="table-cards">
					<thead>
						<tr><th scope="col">Subprocessor</th><th scope="col">Purpose</th><th scope="col">Location</th></tr
						>
					</thead>
					<tbody>
						{#each subprocessors as s (s.name)}
							<tr
								><th scope="row">{s.name}</th><td data-label="Purpose">{s.purpose}</td><td
									data-label="Location">{s.location}</td
								></tr
							>
						{/each}
					</tbody>
				</table>
			</div>
			<p>
				GitHub stores the customer’s crates in the customer’s own organisation, under the customer’s own
				agreement with GitHub. <Todo
					>decide whether GitHub is listed as a subprocessor or as the customer’s own processor</Todo
				>
			</p>

			<h2 id="security">5. Security measures</h2>
			<p>
				The measures in force are described on the <a href="/trust#security">trust centre</a>. In summary:
			</p>
			<ul>
				<li>No durable storage of customer data; in-memory caches keyed by token hashes.</li>
				<li>Tokens never logged, stored or sent anywhere but GitHub’s API.</li>
				<li>Least-privilege GitHub Apps; no repository administration permission.</li>
				<li>HTTPS only with HSTS; an encrypted, <code>HttpOnly</code> session cookie.</li>
				<li>Immutable releases, GitHub-signed provenance and an open-source verifier the customer runs.</li>
				<li>
					Secrets held as encrypted environment variables at the hosting provider, with rotation procedures.
				</li>
			</ul>

			<h2 id="breach">6. Personal data breach</h2>
			<p>
				The processor will notify the customer without undue delay after becoming aware of a personal data
				breach affecting the customer’s data, and in any case within <Todo
					>notification deadline in hours</Todo
				>. Notice goes to the customer’s billing email address and to <Todo
					>any other contact the customer names</Todo
				>. It will describe the breach, the data and people likely affected, the likely consequences, and the
				measures taken or proposed. Service incidents are also posted on the status page.
			</p>

			<h2 id="deletion">7. Deletion and return</h2>
			<ul>
				<li>
					<strong>Registry data</strong> (crates, index and settings) is in the customer’s own GitHub repository
					throughout. The customer keeps it, or deletes it, directly; uninstalling the PrivateCrates GitHub Apps
					ends the processor’s access.
				</li>
				<li>
					<strong>Caches</strong> are in memory only and are lost on restart; access caches expire within 5 minutes.
				</li>
				<li>
					<strong>Logs</strong> age out under the hosting provider’s retention period <Todo
						>confirm the period</Todo
					>.
				</li>
				<li>
					<strong>Billing records</strong> at Stripe are kept as the law requires <Todo
						>retention period for invoices and billing records</Todo
					>.
				</li>
			</ul>

			<h2 id="transfers">8. International transfers</h2>
			<p>
				The service runs in the United States, in Railway’s US East (Virginia) region.
				<Todo>transfer mechanism, e.g. the EU Standard Contractual Clauses and the UK addendum</Todo>
			</p>

			<h2 id="general">9. General</h2>
			<p><Todo>liability, governing law, jurisdiction, and order of precedence with the terms</Todo></p>
			<p>
				Questions about this agreement: <Todo>privacy or legal contact</Todo>. Security questions:
				<a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>.
			</p>
		</TocLayout>
	</div>
</div>

<style>
	.legal {
		padding-top: 2.5rem;
	}
	.intro {
		margin-bottom: 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
	}
</style>
