<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { OPERATOR } from '$lib/site';
	import { CONTACT_EMAIL } from '$lib/trust';
</script>

<Seo
	title="Privacy notice"
	description="What personal data PrivateCrates processes during the preview, why, for how long, who else handles it, and your rights."
	path="/legal/privacy"
/>

<div class="page">
	<article class="prose legal">
		<h1>Privacy notice</h1>
		<p class="lede">
			For the preview. It will be reviewed by a lawyer and reissued by the company that runs PrivateCrates
			before general availability.
		</p>

		<h2>1. Controller</h2>
		<p>
			During the preview, PrivateCrates is run by {OPERATOR}, as an individual, who decides how the personal
			data below is used. A company will take this over before general availability. For anything about your
			data, email <a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a>.
		</p>

		<h2>2. What we process</h2>
		<ul>
			<li>
				<strong>GitHub identity:</strong> your GitHub login, name and avatar, and the organisations you belong to
				with your role, read from GitHub when you sign in.
			</li>
			<li>
				<strong>Tokens:</strong> GitHub tokens sent with registry requests are used only to ask GitHub what you
				can access. They are never stored or logged; caches are keyed by a hash of the token.
			</li>
			<li>
				<strong>Session:</strong> after signing in on this website, your GitHub token is kept only in an encrypted
				cookie in your browser, valid for 8 hours. Nothing is stored on our side.
			</li>
			<li>
				<strong>Terms acceptance:</strong> when an admin accepts the terms for an organisation, we record the organisation’s
				GitHub ID and login, the admin’s GitHub ID and login, the terms version, the time, whether they accepted
				on the website or with the CLI, and the exact statement accepted. It is stored in a Postgres database at
				Railway (US East), with Railway’s backups, and is never shared.
			</li>
			<li>
				<strong>Invitation requests:</strong> while the preview was by invitation, until 6 October 2026, people
				who asked to join it gave their GitHub ID and login, the organisation they named, an email address and a
				note, which are in the same database. We use them only to tell those people that the preview is open, and
				never share them. We no longer collect them.
			</li>
			<li>
				<strong>Billing:</strong> we count your organisation’s active members through GitHub, to tell whether it
				is on the free plan; we keep the count for up to 24 hours and not the members’ names. Stripe processes payment
				details; we receive the subscription status and the organisation it belongs to. Billing is off during the
				preview, so nothing is sent to Stripe.
			</li>
			<li>
				<strong>Request logs:</strong> our hosting provider records each request’s IP address, time, path and browser
				or Cargo version; our own logs record GitHub logins, organisation names, request paths and error codes,
				never tokens.
			</li>
			<li>
				<strong>Crates:</strong> crate files pass through the service while being published and are stored in your
				organisation’s own repository.
			</li>
		</ul>

		<h2>3. Cookies</h2>
		<p>
			One strictly necessary cookie, <code>pc_session</code>, holds your encrypted sign-in. This website uses
			no analytics or advertising cookies. Your theme choice is kept in your browser’s local storage.
		</p>

		<h2>4. Processors</h2>
		<ul>
			<li>GitHub, which stores your crates and provides identity.</li>
			<li>Stripe, for payments, from general availability; not used during the preview.</li>
			<li>
				Railway, which hosts the service, its logs and the database of terms acceptances and invitation
				requests.
			</li>
			<li>
				Cloudflare, for DNS, the status page, and forwarding email sent to our privatecrates.dev addresses.
			</li>
		</ul>

		<h2>5. Retention</h2>
		<p>
			Service logs are deleted after 30 days. Terms acceptances are kept for as long as the organisation uses
			PrivateCrates, then for 6 years, as evidence of the agreement. Invitation requests are deleted once we
			have told you the preview is open, and at the latest 12 months after you asked. Nothing else about you
			is stored by us: caches live in memory and expire within minutes. No billing records exist during the
			preview.
		</p>

		<h2>6. Your rights</h2>
		<p>
			Depending on where you live (for example under the UK or EU GDPR), you can ask for a copy of the
			personal data we hold about you, have it corrected or deleted, restrict or object to its use, and
			receive it in a portable form. Email <a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a> from, or naming,
			your GitHub account; we answer within one month. We use your data because it is needed to run the service
			you or your organisation asked for, and, for terms acceptances, to keep evidence of the agreement. For an
			invitation request, we use your data because you asked us to consider an invitation, and only to answer it.
			A terms acceptance made on behalf of an organisation is kept for its retention period even after a deletion
			request, because it is the record of that agreement.
		</p>
		<p>
			You can also complain to a data protection authority: in the UK, the Information Commissioner’s Office (<a
				href="https://ico.org.uk/make-a-complaint/">ico.org.uk</a
			>); in the EU, the authority where you live or work.
		</p>

		<h2>7. International transfers</h2>
		<p>
			The service runs in the United States (Railway, US East), and GitHub and Cloudflare process data in the
			United States and elsewhere. If you are outside the United States, your data is transferred there so
			that the service you asked for can run. Formal transfer terms (the EU Standard Contractual Clauses and
			the UK addendum) will come with the data processing agreement before general availability.
		</p>
	</article>
</div>

<style>
	.legal {
		padding-top: 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
	}
</style>
