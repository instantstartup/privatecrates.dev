<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import { COMPANY, GA_TARGET, OPERATOR, PRICE_CAP_USD, TERMS_DATE, TERMS_VERSION } from '$lib/site';
	import { CONTACT_EMAIL, SECURITY_EMAIL, STATUS_URL } from '$lib/trust';

	// The ids are stable: the account page and agents link to them.
	const toc: TocItem[] = [
		['operator', '1. Who runs PrivateCrates'],
		['who', '2. Who can use it'],
		['accepting', '3. Accepting these terms'],
		['free', '4. Free during the preview'],
		['your-data', '5. Your data'],
		['acceptable-use', '6. Acceptable use'],
		['as-is', '7. As is: no warranty or guarantee'],
		['availability', '8. Availability and support'],
		['dependencies', '9. Services we depend on'],
		['liability', '10. Liability'],
		['changes', '11. Changes, suspension and the end'],
		['stopping', '12. Stopping'],
		['new-versions', '13. New versions of these terms'],
		['privacy', '14. Privacy and data processing'],
		['law', '15. Law and courts'],
		['contact', '16. Contact']
	];
</script>

<Seo
	title="Preview terms"
	description="The terms for the PrivateCrates preview: open to any GitHub organisation, free, provided as is with no warranty or guarantee, and at your own risk, operated by INSTANTSTARTUP LTD."
	path="/legal/terms"
/>

<div class="page">
	<div class="legal">
		<header class="prose intro">
			<h1>Preview terms</h1>
			<dl class="meta">
				<div>
					<dt>Version</dt>
					<dd><code>{TERMS_VERSION}</code></dd>
				</div>
				<div>
					<dt>Published</dt>
					<dd>{TERMS_DATE}</dd>
				</div>
				<div>
					<dt>Operator</dt>
					<dd>{OPERATOR}, company number {COMPANY.number}</dd>
				</div>
			</dl>
			<p class="lede">
				PrivateCrates is in preview, open to any GitHub organisation or personal account. These terms apply to
				every organisation that uses it until general availability. They are short on purpose; the summary
				below is part of them.
			</p>
		</header>

		<section class="summary panel" aria-labelledby="summary-title">
			<h2 id="summary-title">In short</h2>
			<ul>
				<li>
					<strong>Open to everyone, at our discretion.</strong> Using the preview gives no right to the service:
					we may refuse or end any organisation’s use.
				</li>
				<li><strong>Free.</strong> No payments are taken during the preview.</li>
				<li>
					<strong>As is, at your own risk.</strong> No warranty or guarantee of any kind, no uptime or support commitment,
					and no liability beyond what the law does not let us exclude.
				</li>
				<li>
					<strong>Built on GitHub and others.</strong> When a service we depend on fails, PrivateCrates may too,
					and we are not responsible for it.
				</li>
				<li>
					<strong>Your crates stay yours,</strong> in a repository your GitHub organisation owns, not with us.
				</li>
			</ul>
		</section>

		<div class="body">
			<TocLayout {toc}>
				<h2 id="operator">1. Who runs PrivateCrates</h2>
				<p>
					PrivateCrates is operated by <strong>{OPERATOR}</strong> (“we” and “us” in these terms), a private
					company limited by shares, registered in {COMPANY.registeredIn} with company number {COMPANY.number}.
					Its registered office is {COMPANY.registeredOffice}.
				</p>
				<p>
					“You” means the GitHub organisation that uses PrivateCrates, and the people who use its registry.
				</p>

				<h2 id="who">2. Who can use it</h2>
				<p>
					During the preview, an admin of any GitHub organisation, or anyone for their personal GitHub
					account, may set up a registry on the <a href="/account">account page</a>.
				</p>
				<p>
					Using the preview is permission to try the service, not a promise to provide it. We may refuse a
					registry, or end an organisation’s use of the service, at our discretion and without giving a reason
					(section 11).
				</p>

				<h2 id="accepting">3. Accepting these terms</h2>
				<p>
					An admin of your GitHub organisation accepts these terms on its behalf before a registry is created:
					on the <a href="/account">account page</a>, or by passing
					<code>--accept-terms {TERMS_VERSION}</code> to <code>cargo privatecrates setup</code>. By accepting,
					the admin confirms they have read these terms and may agree to them for the organisation.
				</p>
				<p>
					We keep a record of the acceptance, as evidence of the agreement: the organisation, the admin’s
					GitHub account, this version, the time, whether it was accepted on the website or with the CLI, and
					the exact statement accepted. It is what we store durably about your organisation; the
					<a href="/trust#what-we-hold">trust centre</a> says where and for how long.
				</p>
				<p>
					An AI agent setting up a registry must show these terms to the admin and wait for them to accept. It
					must never accept on their behalf.
				</p>

				<h2 id="free">4. Free during the preview</h2>
				<p>
					The preview costs nothing. We take no payments and ask for no card, whatever the size of your
					organisation.
				</p>
				<p>
					The <a href="/pricing">pricing page</a> shows what we plan to charge from general availability,
					planned for {GA_TARGET}: free for small organisations, and at most ${PRICE_CAP_USD} per organisation per
					month for larger ones. Billing starts only after general availability, and only after at least 30 days’
					notice. Paying will need new terms, and no charge is ever taken without a card that an admin has added.
				</p>

				<h2 id="your-data">5. Your data</h2>
				<p>
					Your registry’s index, settings and every crate file are stored in a private repository in your own
					GitHub organisation, under your agreement with GitHub. They belong to you. You let us read and write
					that repository through the storage App only to run your registry.
				</p>
				<p>
					We keep no copy of your crates, index or code: only caches in memory, service logs kept for 30 days,
					and the record of your organisation accepting these terms (section 3). The
					<a href="/trust">trust centre</a> lists what we hold, where it runs and who else processes it.
				</p>
				<p>
					Because your data is in your repository, it stays there if you leave or the service ends. Looking
					after that repository, including any backups you want, is up to you and GitHub.
				</p>

				<h2 id="acceptable-use">6. Acceptable use</h2>
				<p>Use PrivateCrates for your organisation’s own Rust crates. Do not use it to:</p>
				<ul>
					<li>break the law, or infringe anyone’s rights;</li>
					<li>distribute malware, or crates meant to harm the people who build them;</li>
					<li>
						reach another organisation’s registry or data, or get round the permissions GitHub gives you;
					</li>
					<li>
						overload the service or GitHub on our behalf, for example by scripting requests far beyond what
						your builds need;
					</li>
					<li>resell the service or offer it to others as your own.</li>
				</ul>
				<p>
					Security research in good faith is welcome under our
					<a href="/security">vulnerability disclosure policy</a>.
				</p>

				<h2 id="as-is">7. As is: no warranty or guarantee</h2>
				<p>
					PrivateCrates is an early, unfinished service. It is provided <strong>as is and as available</strong
					>, and you use it entirely at your own risk.
				</p>
				<p>
					To the fullest extent the law allows, we give <strong
						>no warranty, guarantee, condition or promise of any kind</strong
					>, whether express, implied or statutory. In particular, we do not promise that the service is of
					satisfactory quality, fit for any purpose, accurate, secure, free of errors or vulnerabilities,
					compatible with your tools, or available without interruption, or that it will keep your crates
					private or intact.
				</p>
				<p>
					This website, its documentation, the trust centre and the security pages describe how PrivateCrates
					is designed to work. They are descriptions, not promises, and nothing in them, or anything we say to
					you, creates a warranty or guarantee.
				</p>
				<p>
					You are responsible for deciding whether PrivateCrates is suitable for you, for checking what you
					build and publish with it, and for your own safeguards: run the open-source
					<a href="/docs/verify">verifier</a> on your storage repository, and do not rely on the preview for anything
					you could not cope with losing access to, or with being exposed.
				</p>

				<h2 id="availability">8. Availability and support</h2>
				<p>
					There is no availability commitment and no service level. We post incidents on the
					<a href={STATUS_URL}>status page</a> and fix problems as quickly as we reasonably can, but we promise
					no response or resolution times.
				</p>
				<p>
					Support is best effort. Security problems go to
					<a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a> (section 16).
				</p>

				<h2 id="dependencies">9. Services we depend on</h2>
				<p>
					PrivateCrates is built on services run by other companies, which we do not control. Chief among them
					is GitHub: it stores your registry, signs people in, decides who may read and publish each crate,
					and runs the workflows that publish them. We also depend on Railway, which hosts the service;
					Cloudflare, for DNS and the status page; crates.io, for checking crate names; and, from general
					availability, Stripe, for billing.
				</p>
				<p>
					We are not responsible for these services: for their availability, performance, security or
					correctness, for changes they make, or for anything they do or fail to do. When one of them is down,
					degraded or changed, parts of PrivateCrates, or all of it, may stop working, and there may be
					nothing we can do until it recovers. The <a href={STATUS_URL}>status page</a> shows when a problem is
					theirs.
				</p>
				<p>
					Your use of GitHub, including your storage repository, is under your own agreement with GitHub, not
					these terms.
				</p>

				<h2 id="liability">10. Liability</h2>
				<p>
					To the extent the law allows, we are not liable for any loss or damage arising from the private
					preview or from your use of it, whether direct or indirect, and however it arises, including in
					negligence: including lost profits, revenue, data, or time, the exposure of code or data, harm from
					a crate built or published through the service, and the cost of replacing the service. That includes
					loss or damage caused by a service we depend on (section 9) being unavailable, degraded, changed or
					compromised, or by anything it does or fails to do.
				</p>
				<p>
					Nothing in these terms limits liability that the law does not allow to be limited, such as liability
					for death or personal injury caused by negligence, or for fraud.
				</p>

				<h2 id="changes">11. Changes, suspension and the end of the service</h2>
				<p>
					The preview may change as we build towards general availability: features may be added, changed or
					removed. We may also end the preview, end an organisation’s use of it, or end the service
					altogether. Where we can, we give notice first, on this site and on the status page; an emergency,
					such as a security problem or GitHub withdrawing access, may not allow it.
				</p>
				<p>
					We may suspend or remove an organisation’s registry that breaks section 6, or that threatens the
					service or other organisations, and will tell its admins why where we can. Your storage repository
					is untouched either way.
				</p>

				<h2 id="stopping">12. Stopping</h2>
				<p>
					You can stop at any time: uninstall the PrivateCrates GitHub Apps from your organisation. Your
					storage repository, with your index and every crate file, stays where it is.
				</p>

				<h2 id="new-versions">13. New versions of these terms</h2>
				<p>
					When these terms change, we publish a new version here with a new version name. A new version
					applies to your organisation once an admin accepts it; the account page asks them to. Until then,
					the version your organisation accepted continues to apply. If you do not want to accept a new
					version, you can stop (section 12).
				</p>

				<h2 id="privacy">14. Privacy and data processing</h2>
				<p>
					The <a href="/legal/privacy">privacy notice</a> explains how we handle personal data. The
					<a href="/legal/dpa">data processing agreement</a> is a draft, and is
					<strong>not yet in force during the preview</strong>. How the service handles data today is
					described, factually, on the <a href="/trust">trust centre</a>. Both documents will be reviewed by a
					lawyer before general availability.
				</p>

				<h2 id="law">15. Law and courts</h2>
				<p>
					These terms, and any dispute or claim arising out of or in connection with them or with
					PrivateCrates, including non-contractual ones, are governed by the law of England and Wales. The
					courts of England and Wales have exclusive jurisdiction over them.
				</p>

				<h2 id="contact">16. Contact</h2>
				<ul>
					<li>
						Security problems: <a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>, as described in the
						<a href="/security">disclosure policy</a>.
					</li>
					<li>
						Everything else, including these terms and privacy: <a href="mailto:{CONTACT_EMAIL}"
							>{CONTACT_EMAIL}</a
						>.
					</li>
					<li>Formal notices by post: {OPERATOR}, {COMPANY.registeredOffice}.</li>
				</ul>
			</TocLayout>
		</div>
	</div>
</div>

<style>
	.legal {
		padding-top: 2.5rem;
	}
	h1 {
		font-size: var(--text-headline);
	}
	.intro > * + * {
		margin-top: 1rem;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem 2rem;
		font-size: var(--text-meta);
	}
	.meta dt {
		color: var(--ink-muted);
	}
	.meta dd {
		font-weight: 700;
	}

	/* The summary: the whole agreement at a glance, set apart like the trust centre's manifest. */
	.summary {
		margin-top: 2rem;
		padding: 1.5rem 1.25rem;
		border: 2px solid var(--line);
	}
	@media (min-width: 40rem) {
		.summary {
			padding: 1.75rem 2rem;
		}
	}
	.summary h2 {
		font-size: var(--text-title);
		margin-bottom: 0.75rem;
	}
	.summary ul {
		display: grid;
	}
	.summary li {
		padding-block: 0.6rem;
		border-top: 1px dashed var(--divider);
	}
	.summary li:first-child {
		border-top: 0;
		padding-top: 0;
	}

	.body {
		margin-top: 3rem;
	}
</style>
