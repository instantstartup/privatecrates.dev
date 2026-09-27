<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import type { TocItem } from '$lib/components/Toc.svelte';
	import TocLayout from '$lib/components/TocLayout.svelte';
	import Todo from '$lib/components/Todo.svelte';
	import { GA_TARGET, OPERATOR, PRICE_USD, TERMS_DATE, TERMS_VERSION } from '$lib/site';
	import { SECURITY_EMAIL, STATUS_URL } from '$lib/trust';

	// The ids are stable: the account page and agents link to them.
	const toc: TocItem[] = [
		['operator', '1. Who runs PrivateCrates'],
		['accepting', '2. Accepting these terms'],
		['free', '3. Free during the preview'],
		['your-data', '4. Your data'],
		['acceptable-use', '5. Acceptable use'],
		['as-is', '6. As is, at your own risk'],
		['availability', '7. Availability and support'],
		['liability', '8. Liability'],
		['changes', '9. Changes, suspension and the end'],
		['stopping', '10. Stopping'],
		['new-versions', '11. New versions of these terms'],
		['privacy', '12. Privacy and data processing'],
		['contact', '13. Contact']
	];
</script>

<Seo
	title="Preview terms"
	description="The terms for the PrivateCrates preview: free, provided as is and at your own risk, run by an individual until a company is formed. Your crates stay in your own GitHub organisation."
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
					<dd>{OPERATOR}, an individual</dd>
				</div>
			</dl>
			<p class="lede">
				PrivateCrates is in preview. These terms apply to every organisation that uses it until general
				availability. They are short on purpose; the summary below is part of them.
			</p>
		</header>

		<section class="summary panel" aria-labelledby="summary-title">
			<h2 id="summary-title">In short</h2>
			<ul>
				<li><strong>Free.</strong> No payments are taken during the preview.</li>
				<li>
					<strong>As is, at your own risk.</strong> No warranty, no uptime or support commitment, and no liability
					beyond what the law does not let us exclude.
				</li>
				<li>
					<strong>Your crates stay yours,</strong> in a repository your GitHub organisation owns. If the service
					ends, you lose nothing stored there.
				</li>
				<li>
					<strong>Run by one person</strong> for now. A company takes over before general availability, with new
					terms for you to accept.
				</li>
			</ul>
		</section>

		<div class="body">
			<TocLayout {toc}>
				<h2 id="operator">1. Who runs PrivateCrates</h2>
				<p>
					PrivateCrates is operated by <strong>{OPERATOR}, an individual</strong> (“we” and “us” in these terms).
					There is no company yet. A company will be formed before general availability, and its name, number and
					address will be published here. Moving the service to that company will come with new terms, which you
					can accept or decline (section 11).
				</p>
				<p>
					“You” means the GitHub organisation that uses PrivateCrates, and the people who use its registry.
				</p>

				<h2 id="accepting">2. Accepting these terms</h2>
				<p>
					An admin of your GitHub organisation accepts these terms on its behalf before a registry is created:
					on the <a href="/account">account page</a>, or by passing
					<code>--accept-terms {TERMS_VERSION}</code> to <code>cargo privatecrates setup</code>. By accepting,
					the admin confirms they have read these terms and may agree to them for the organisation.
				</p>
				<p>
					We keep a record of the acceptance, as evidence of the agreement: the organisation, the admin’s
					GitHub account, this version, the time, whether it was accepted on the website or with the CLI, and
					the exact statement accepted. It is the one thing we store durably; the
					<a href="/trust#what-we-hold">trust centre</a> says where and for how long.
				</p>
				<p>
					An AI agent setting up a registry must show these terms to the admin and wait for them to accept. It
					must never accept on their behalf.
				</p>

				<h2 id="free">3. Free during the preview</h2>
				<p>
					The preview costs nothing. We take no payments and ask for no card, whatever the size of your
					organisation.
				</p>
				<p>
					The <a href="/pricing">pricing page</a> shows what we plan to charge from general availability,
					planned for {GA_TARGET}: free for small organisations, ${PRICE_USD} per organisation per month for larger
					ones. Billing starts only after general availability, and only after at least 30 days’ notice. Paying
					will need new terms with the company, and no charge is ever taken without a card that an admin has added.
				</p>

				<h2 id="your-data">4. Your data</h2>
				<p>
					Your registry’s index, settings and every crate file are stored in a private repository in your own
					GitHub organisation, under your agreement with GitHub. They belong to you. You let us read and write
					that repository through the storage App only to run your registry.
				</p>
				<p>
					We keep no copy of your crates, index or code: only caches in memory, service logs kept for 30 days,
					and the record of your organisation accepting these terms (section 2). The
					<a href="/trust">trust centre</a> lists what we hold, where it runs and who else processes it.
				</p>
				<p>
					Because everything is in your repository, the service ending, or you leaving, loses nothing stored
					there.
				</p>

				<h2 id="acceptable-use">5. Acceptable use</h2>
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

				<h2 id="as-is">6. As is, at your own risk</h2>
				<p>
					PrivateCrates is a preview. It is provided <strong>as is and as available</strong>, and you use it
					at your own risk. We make no promise, express or implied, that it is fit for any particular purpose,
					error-free, secure or uninterrupted, to the extent the law allows us to exclude such promises.
				</p>
				<p>
					Keep your own safeguards: run the open-source <a href="/docs/verify">verifier</a> on your storage repository,
					and do not rely on the preview for anything you could not cope with losing access to for a while.
				</p>

				<h2 id="availability">7. Availability and support</h2>
				<p>
					There is no availability commitment and no service level. We post incidents on the
					<a href={STATUS_URL}>status page</a> and fix problems as quickly as we reasonably can, but we promise
					no response or resolution times.
				</p>
				<p>
					Support is best effort. Security problems go to
					<a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a> (section 13).
				</p>

				<h2 id="liability">8. Liability</h2>
				<p>
					To the extent the law allows, we are not liable for any loss or damage arising from the preview or
					from your use of it, whether direct or indirect: including lost profits, revenue, data, or time, and
					the cost of replacing the service.
				</p>
				<p>
					Nothing in these terms limits liability that the law does not allow to be limited, such as liability
					for death or personal injury caused by negligence, or for fraud.
				</p>

				<h2 id="changes">9. Changes, suspension and the end of the service</h2>
				<p>
					The preview may change as we build towards general availability: features may be added, changed or
					removed. We may also end the preview, or the service altogether. Where we can, we give notice first,
					on this site and on the status page; an emergency, such as a security problem or GitHub withdrawing
					access, may not allow it.
				</p>
				<p>
					We may suspend or remove an organisation’s registry that breaks section 5, or that threatens the
					service or other organisations, and will tell its admins why where we can. Your storage repository
					is untouched either way.
				</p>

				<h2 id="stopping">10. Stopping</h2>
				<p>
					You can stop at any time: uninstall the PrivateCrates GitHub Apps from your organisation. Your
					storage repository, with your index and every crate file, stays where it is.
				</p>

				<h2 id="new-versions">11. New versions of these terms</h2>
				<p>
					When these terms change, we publish a new version here with a new version name. A new version
					applies to your organisation once an admin accepts it; the account page asks them to. Until then,
					the version your organisation accepted continues to apply. If you do not want to accept a new
					version, you can stop (section 10).
				</p>

				<h2 id="privacy">12. Privacy and data processing</h2>
				<p>
					The <a href="/legal/privacy">privacy notice</a> and the
					<a href="/legal/dpa">data processing agreement</a> are drafts, and are
					<strong>not yet in force during the preview</strong>. How the service handles data today is
					described, factually, on the <a href="/trust">trust centre</a>. Both documents will be reviewed and
					put in force before general availability.
				</p>

				<h2 id="contact">13. Contact</h2>
				<ul>
					<li>
						Security problems: <a href="mailto:{SECURITY_EMAIL}">{SECURITY_EMAIL}</a>, as described in the
						<a href="/security">disclosure policy</a>.
					</li>
					<li>Everything else, including these terms: <Todo>general contact email address</Todo>.</li>
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
		font-size: var(--text-3xl);
	}
	.intro > * + * {
		margin-top: 1rem;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem 2rem;
		font-size: var(--text-sm);
	}
	.meta dt {
		color: var(--ink-soft);
	}
	.meta dd {
		font-weight: 700;
	}

	/* The summary: the whole agreement at a glance, set apart like the trust centre's manifest. */
	.summary {
		margin-top: 2rem;
		padding: 1.5rem 1.25rem;
		border: 2px solid var(--ink);
	}
	@media (min-width: 40rem) {
		.summary {
			padding: 1.75rem 2rem;
		}
	}
	.summary h2 {
		font-size: var(--text-xl);
		margin-bottom: 0.75rem;
	}
	.summary ul {
		display: grid;
	}
	.summary li {
		padding-block: 0.6rem;
		border-top: 1px dashed var(--rule);
	}
	.summary li:first-child {
		border-top: 0;
		padding-top: 0;
	}

	.body {
		margin-top: 3rem;
	}
</style>
