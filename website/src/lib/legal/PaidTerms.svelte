<script lang="ts">
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import { REFUNDS_PATH } from '$lib/legal/docs';
	import {
		COMPANY,
		FREE_MEMBER_LIMIT,
		MEMBER_PRICE_USD,
		OPERATOR,
		PRICE_CAP_USD,
		READ_GRACE_DAYS,
		TERMS_VERSION,
		TRIAL_MONTHS,
		TRIAL_REMINDER_DAYS
	} from '$lib/site';
	import { CONTACT_EMAIL, SECURITY_EMAIL, STATUS_URL } from '$lib/trust';

	// The terms, the only ones there are. The trust centre and the docs link to #dependencies.
	const toc: TocItem[] = [
		['operator', '1. Who runs PrivateCrates'],
		['who', '2. Who can use it'],
		['accepting', '3. Accepting these terms'],
		['plans', '4. Plans and prices'],
		['trial', '5. The free trial'],
		['billing', '6. Billing and payment'],
		['cancelling', '7. Cancelling and refunds'],
		['your-data', '8. Your data'],
		['acceptable-use', '9. Acceptable use'],
		['service', '10. The service and support'],
		['dependencies', '11. Services we depend on'],
		['liability', '12. Liability'],
		['changes', '13. Changes, suspension and the end'],
		['stopping', '14. Stopping'],
		['new-versions', '15. New versions of these terms'],
		['privacy', '16. Privacy and data processing'],
		['general', '17. General'],
		['law', '18. Law and courts'],
		['contact', '19. Contact']
	];
</script>

<TocLayout {toc}>
	<h2 id="operator">1. Who runs PrivateCrates</h2>
	<p>
		PrivateCrates is operated by <strong>{OPERATOR}</strong> (“we” and “us” in these terms), a private company
		limited by shares, registered in {COMPANY.registeredIn} with company number {COMPANY.number}. Its
		registered office is {COMPANY.registeredOffice}.
	</p>
	<p>
		“You” means the GitHub organisation, or the person with a personal GitHub account, that uses
		PrivateCrates, and the people who use its registry.
	</p>

	<h2 id="who">2. Who can use it</h2>
	<p>
		An admin of any GitHub organisation, or anyone for their personal GitHub account, may set up a registry on
		the
		<a href="/account">account page</a>.
	</p>
	<p>
		PrivateCrates is a service for businesses and other organisations, and for developers using it for their
		work. By accepting these terms you confirm that you are not using it as a consumer.
	</p>

	<h2 id="accepting">3. Accepting these terms</h2>
	<p>
		An admin of your GitHub organisation accepts these terms on its behalf before a registry is created: on
		the
		<a href="/account">account page</a>, or by passing <code>--accept-terms {TERMS_VERSION}</code> to
		<code>cargo privatecrates setup</code>. By accepting, the admin confirms they have read these terms and
		may agree to them for the organisation.
	</p>
	<p>
		We keep a record of the acceptance, as evidence of the agreement: the organisation, the admin’s GitHub
		account, this version, the time, whether it was accepted on the website or with the CLI, and the exact
		statement accepted. The <a href="/trust#what-we-hold">trust centre</a> says where and for how long.
	</p>
	<p>
		An AI agent setting up a registry must show these terms to the admin and wait for them to accept. It must
		never accept on their behalf.
	</p>

	<h2 id="plans">4. Plans and prices</h2>
	<ul>
		<li>
			<strong>Free:</strong> personal GitHub accounts, and organisations with {FREE_MEMBER_LIMIT} active members
			or fewer.
		</li>
		<li>
			<strong>Team:</strong> organisations with more than {FREE_MEMBER_LIMIT} active members pay ${MEMBER_PRICE_USD}
			a month for each member past {FREE_MEMBER_LIMIT}, and never more than ${PRICE_CAP_USD} a month, however many
			members they have.
		</li>
	</ul>
	<p>
		Members are your GitHub organisation’s active members, as GitHub lists them. Outside collaborators,
		pending invitations and CI jobs do not count. We count them through GitHub and keep the count current as
		members join and leave. Both plans have every feature, with unlimited users. The <a href="/pricing"
			>pricing page</a
		> shows the current prices.
	</p>
	<p>
		Prices are in US dollars. They do not include sales tax or VAT; where we must charge tax, it is added and
		shown on the invoice.
	</p>

	<h2 id="trial">5. The free trial</h2>
	<p>
		An organisation that grows past {FREE_MEMBER_LIMIT} members gets a {TRIAL_MONTHS}-month free trial, with
		no card needed. It starts by itself when the organisation first grows past the limit, or with one click by
		an admin of an organisation that is already larger. Each organisation gets one trial.
	</p>
	<p>
		During the trial everything works as on the paid plan. From {TRIAL_REMINDER_DAYS} days before it ends, until
		an admin adds a card, the account page and every <code>cargo publish</code> say when it ends and what you will
		pay after it.
	</p>
	<p>When the trial ends:</p>
	<ul>
		<li>
			<strong>with a card added,</strong> the subscription continues and the first monthly payment is taken;
		</li>
		<li>
			<strong>without a card,</strong> the subscription ends and nothing is charged. Publishing stops at once;
			reading crates keeps working for {READ_GRACE_DAYS} days, then stops too. An admin can subscribe at any time
			to turn the registry back on.
		</li>
	</ul>
	<p>
		If a trial or subscription has ended and your organisation then has {FREE_MEMBER_LIMIT} members or fewer, growing
		past the limit again later starts a 14-day grace period with no card instead of a second trial, on the same
		terms.
	</p>

	<h2 id="billing">6. Billing and payment</h2>
	<ul>
		<li>
			<strong>Monthly, in advance.</strong> After the trial, or from the day you subscribe if you have had one,
			we charge each month at the start of the month, to the card an admin added. We never charge without a card
			an admin has added.
		</li>
		<li>
			<strong>Stripe</strong> takes the payments and sends invoices and receipts to the billing email an admin gives
			us. Card details go to Stripe; we never see them. Admins update the card, the billing email and download invoices
			under Manage billing on the account page.
		</li>
		<li>
			<strong>Changes in members</strong> are billed from the next invoice, without proration: a member who
			joins mid-month is not charged for that month, and a member who leaves is not refunded for it. An
			organisation that shrinks to {FREE_MEMBER_LIMIT} members or fewer pays nothing from its next invoice; its
			subscription stays, at no cost, so nothing needs doing if it grows again.
		</li>
		<li>
			<strong>A failed payment</strong> does not stop the registry at once: Stripe retries it over the
			following days, and the account page asks an admin to update the card. If it still fails, the
			subscription ends, and the registry stops as when a trial ends without a card (section 5): publishing at
			once, reading
			{READ_GRACE_DAYS} days later.
		</li>
		<li>
			<strong>Price changes</strong> are announced at least 30 days ahead, by email to the billing email and on
			the pricing page, and apply from the first invoice after the notice period. You can cancel before then (section
			7).
		</li>
	</ul>

	<h2 id="cancelling">7. Cancelling and refunds</h2>
	<p>
		An admin can cancel at any time, under Manage billing on the <a href="/account">account page</a>.
		Cancelling takes effect at the end of the month already paid for: the registry works normally until then,
		nothing more is charged, and reading keeps working for {READ_GRACE_DAYS} days after it.
	</p>
	<p>
		Payments are not refunded for part of a month, including when you cancel, remove members or shrink to the
		free plan. If we charged you in error, we refund it. The <a href={REFUNDS_PATH}
			>refunds and cancellation</a
		> page sets this out in full.
	</p>
	<p>
		Uninstalling the PrivateCrates GitHub Apps stops the registry but does not cancel the subscription: cancel
		it too.
	</p>

	<h2 id="your-data">8. Your data</h2>
	<p>
		Your registry’s index, settings and every crate file are stored in a private repository in your own GitHub
		organisation, under your agreement with GitHub. They belong to you. You let us read and write that
		repository through the storage App only to run your registry.
	</p>
	<p>
		We keep no copy of your crates, index or code: only caches in memory, service logs kept for 30 days, and
		the record of your organisation accepting these terms (section 3). Stripe keeps your billing records. The
		<a href="/trust">trust centre</a> lists what we hold, where it runs and who else processes it.
	</p>
	<p>
		Because your data is in your repository, it stays there if you cancel, leave or the service ends. Looking
		after that repository, including any backups you want, is up to you and GitHub.
	</p>

	<h2 id="acceptable-use">9. Acceptable use</h2>
	<p>Use PrivateCrates for your organisation’s own Rust crates. Do not use it to:</p>
	<ul>
		<li>break the law, or infringe anyone’s rights;</li>
		<li>distribute malware, or crates meant to harm the people who build them;</li>
		<li>reach another organisation’s registry or data, or get round the permissions GitHub gives you;</li>
		<li>
			overload the service or GitHub on our behalf, for example by scripting requests far beyond what your
			builds need;
		</li>
		<li>avoid paying, for example by splitting one team across organisations to stay on the free plan;</li>
		<li>resell the service or offer it to others as your own.</li>
	</ul>
	<p>
		Security research in good faith is welcome under our <a href="/security"
			>vulnerability disclosure policy</a
		>.
	</p>

	<h2 id="service">10. The service and support</h2>
	<p>
		We provide PrivateCrates with reasonable care and skill, and keep it secure as described on the
		<a href="/trust">trust centre</a>. Apart from that, and to the extent the law allows, we give no other
		warranty, guarantee or condition, express or implied: in particular, we do not promise that the service
		will be uninterrupted or free of errors, or that it suits a particular purpose.
	</p>
	<p>
		There is no service level agreement. We post incidents on the <a href={STATUS_URL}>status page</a> and fix
		problems as quickly as we reasonably can. Support is by email, on working days in the UK, at
		<a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a>; security problems go to
		<a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>.
	</p>
	<p>
		You are responsible for checking what you build and publish with PrivateCrates. The open-source
		<a href="/docs/verify">verifier</a> lets you check everything the service writes to your storage repository.
	</p>

	<h2 id="dependencies">11. Services we depend on</h2>
	<p>
		PrivateCrates is built on services run by other companies, which we do not control. Chief among them is
		GitHub: it stores your registry, signs people in, decides who may read and publish each crate, and runs
		the workflows that publish them. We also depend on Railway, which hosts the service; Cloudflare, for DNS
		and the status page; crates.io, for checking crate names; and Stripe, for billing.
	</p>
	<p>
		We are not responsible for these services: for their availability, performance, security or correctness,
		for changes they make, or for anything they do or fail to do. When one of them is down, degraded or
		changed, parts of PrivateCrates, or all of it, may stop working until it recovers. The <a
			href={STATUS_URL}>status page</a
		> shows when a problem is theirs.
	</p>
	<p>
		Your use of GitHub, including your storage repository, is under your own agreement with GitHub, not these
		terms.
	</p>

	<h2 id="liability">12. Liability</h2>
	<p>To the extent the law allows:</p>
	<ul>
		<li>
			we are not liable for indirect or consequential loss, or for loss of profits, revenue, business,
			goodwill, data or time, however it arises, including in negligence;
		</li>
		<li>
			we are not liable for loss caused by a service we depend on (section 11) being unavailable, degraded,
			changed or compromised;
		</li>
		<li>
			our total liability to your organisation for everything arising out of or in connection with these terms
			or PrivateCrates, in any 12 months, is limited to the greater of the amount your organisation paid us in
			the 12 months before the claim and US$100.
		</li>
	</ul>
	<p>
		Nothing in these terms limits liability that the law does not allow to be limited, such as liability for
		death or personal injury caused by negligence, or for fraud.
	</p>

	<h2 id="changes">13. Changes, suspension and the end of the service</h2>
	<p>
		PrivateCrates will change as we improve it. We tell you at least 30 days ahead, by email to the billing
		email, before a change that takes away something your organisation relies on, unless security or a service
		we depend on forces it sooner.
	</p>
	<p>
		We may suspend or end an organisation’s registry that breaks section 9, that threatens the service or
		other organisations, or whose payments have failed (section 6). We tell its admins why where we can. Your
		storage repository is untouched either way.
	</p>
	<p>
		We may stop offering PrivateCrates altogether, with at least 30 days’ notice. If we do, or if we end your
		organisation’s use for any reason other than a breach of these terms, we refund what you paid for the time
		after it ends.
	</p>

	<h2 id="stopping">14. Stopping</h2>
	<p>
		You can stop at any time: cancel the subscription, if you have one (section 7), and uninstall the
		PrivateCrates GitHub Apps from your organisation. Your storage repository, with your index and every crate
		file, stays where it is.
	</p>

	<h2 id="new-versions">15. New versions of these terms</h2>
	<p>
		When these terms change, we publish a new version here with a new version name, and tell organisations
		that pay at least 30 days ahead, by email to the billing email. A new version applies to your organisation
		once an admin accepts it; the account page asks them to. Until then, the version your organisation
		accepted continues to apply. If you do not want to accept a new version, you can stop (section 14).
	</p>

	<h2 id="privacy">16. Privacy and data processing</h2>
	<p>
		The <a href="/legal/privacy">privacy notice</a> explains how we handle personal data. Where we process
		personal data on your organisation’s behalf, the <a href="/legal/dpa">data processing agreement</a> applies
		and forms part of these terms.
	</p>

	<h2 id="general">17. General</h2>
	<ul>
		<li>
			These terms, with the documents they refer to, are the whole agreement between us about PrivateCrates.
		</li>
		<li>
			We may transfer our rights and obligations under them to a company that takes over PrivateCrates, and
			will tell you if we do. You may transfer yours only with our written agreement.
		</li>
		<li>Nobody else has a right to enforce these terms.</li>
		<li>If a court finds part of these terms unenforceable, the rest still applies.</li>
		<li>Neither of us is liable for a delay or failure caused by events beyond our reasonable control.</li>
	</ul>

	<h2 id="law">18. Law and courts</h2>
	<p>
		These terms, and any dispute or claim arising out of or in connection with them or with PrivateCrates,
		including non-contractual ones, are governed by the law of England and Wales. The courts of England and
		Wales have exclusive jurisdiction over them.
	</p>

	<h2 id="contact">19. Contact</h2>
	<ul>
		<li>
			Security problems: <a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>, as described in the
			<a href="/security">disclosure policy</a>.
		</li>
		<li>
			Billing, refunds, these terms, privacy and everything else:
			<a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a>.
		</li>
		<li>Formal notices by post: {OPERATOR}, {COMPANY.registeredOffice}.</li>
	</ul>
</TocLayout>
