<script lang="ts">
	import LegalMeta from '$lib/components/LegalMeta.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { PRIVACY_DOC } from '$lib/legal/docs';
	import { COMPANY, OPERATOR, PREVIEW } from '$lib/site';
	import { CONTACT_EMAIL } from '$lib/trust';
</script>

<Seo
	title="Privacy notice"
	description={PREVIEW
		? 'What personal data PrivateCrates processes during the preview, why, for how long, who else handles it, and your rights.'
		: 'What personal data PrivateCrates processes, including for billing with Stripe, why, for how long, who else handles it, and your rights.'}
	path="/legal/privacy"
/>

<div class="page">
	<article class="prose legal">
		<h1>Privacy notice</h1>
		{#if PREVIEW}
			<p class="lede">For the preview. It will be reviewed by a lawyer before general availability.</p>
		{:else}
			<LegalMeta version={PRIVACY_DOC.version} date={PRIVACY_DOC.date} />
		{/if}

		<h2>1. Controller</h2>
		<p>
			PrivateCrates is run by {OPERATOR}, company number {COMPANY.number}, registered in {COMPANY.registeredIn},
			of {COMPANY.registeredOffice}. It decides how the personal data below is used. For anything about your
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
				<strong>Billing:</strong> we count your organisation’s active members through GitHub, to tell whether
				it is on the free plan; we keep the count for up to 24 hours and not the members’ names.
				{#if PREVIEW}
					Stripe processes payment details; we receive the subscription status and the organisation it belongs
					to. Billing is off during the preview, so nothing is sent to Stripe.
				{:else}
					For an organisation past the free plan, Stripe receives the billing email address an admin gives us,
					the organisation’s GitHub name and ID, and the number of members it pays for. Card details are
					entered on Stripe’s own pages and go only to Stripe; we never see them. If Stripe asks for a billing
					address or a tax ID, they go to Stripe too. We receive back the subscription’s status and dates, and
					whether a card has been added; we do not store them.
				{/if}
			</li>
			<li>
				<strong>Request logs:</strong> our hosting provider records each request’s IP address, time, path and browser
				or Cargo version; our own logs record GitHub logins, organisation names, crate names, request paths and
				error codes, never tokens. Our traces record what kind of request each was and how long it took, not which
				crate or organisation it named.
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
			{#if PREVIEW}
				<li>Stripe, for payments, from general availability; not used during the preview.</li>
			{:else}
				<li>
					Stripe, which takes payments, keeps billing records, sends invoices and receipts, and runs the
					billing portal. Stripe also uses some payment data as a controller in its own right, to prevent
					fraud and meet its legal obligations, as its <a href="https://stripe.com/privacy">privacy policy</a> explains.
				</li>
			{/if}
			<li>Railway, which hosts the service, its logs and its database.</li>
			<li>
				Cloudflare, for DNS, the status page, and forwarding email sent to our privatecrates.dev addresses.
			</li>
			<li>Grafana Labs (Grafana Cloud, EU region), which keeps our service logs, traces and metrics.</li>
		</ul>

		<h2>5. Retention</h2>
		<p>
			Service logs, at Railway and Grafana Cloud, are deleted after 30 days at the latest. Terms acceptances
			are kept for as long as the organisation uses PrivateCrates, then for 6 years, as evidence of the
			agreement. Nothing else about you is stored by us: caches live in memory and expire within minutes.
			{#if PREVIEW}
				No billing records exist during the preview.
			{:else}
				Billing records (invoices, payments and the billing email) are kept at Stripe for as long as tax law
				requires: 6 years in the UK.
			{/if}
		</p>

		<h2>6. Your rights</h2>
		<p>
			Depending on where you live (for example under the UK or EU GDPR), you can ask for a copy of the
			personal data we hold about you, have it corrected or deleted, restrict or object to its use, and
			receive it in a portable form. Email <a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a> from, or
			naming, your GitHub account; we answer within one month. We use your data because it is needed to run
			the service you or your organisation asked for{#if !PREVIEW}, and to bill for it{/if}, and, for terms
			acceptances, to keep evidence of the agreement.{#if !PREVIEW}
				We keep billing records because tax law requires it.{/if} A terms acceptance made on behalf of an organisation
			is kept for its retention period even after a deletion request, because it is the record of that agreement.
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
			that the service you asked for can run.
			{#if PREVIEW}
				Formal transfer terms (the EU Standard Contractual Clauses and the UK addendum) will come with the
				data processing agreement before general availability.
			{:else}
				Stripe also processes billing data in the United States and elsewhere. These transfers are protected
				by the EU Standard Contractual Clauses (Commission Decision 2021/914) and the UK International Data
				Transfer Addendum, or by the EU–US Data Privacy Framework and its UK extension where the recipient is
				certified under it. For data we process for your organisation, the
				<a href="/legal/dpa#transfers">data processing agreement</a> incorporates the same clauses.
			{/if}
		</p>
	</article>
</div>

<style>
	.legal {
		padding-top: var(--space-10);
	}
	h1 {
		font-size: var(--text-headline);
	}
</style>
