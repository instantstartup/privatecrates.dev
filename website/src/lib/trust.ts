// Facts shared by the trust centre, the security policy and the DPA template, so they cannot drift apart.

/** Where security reports go (also in static/.well-known/security.txt). */
export const SECURITY_EMAIL = 'security@privatecrates.dev';
/** The public status page, a Cloudflare Worker independent of Railway (docs/trust-and-status.md §1). */
export const STATUS_URL = 'https://status.privatecrates.dev';

export interface Subprocessor {
	name: string;
	/** What we use them for. */
	purpose: string;
	/** What customer data they see through us. */
	data: string;
	/** Where the processing happens, as far as we know it. */
	location: string;
	url: string;
}

export const subprocessors: Subprocessor[] = [
	{
		name: 'Railway',
		purpose:
			'Hosts the PrivateCrates service: it runs the server, terminates TLS for our domains, keeps its logs, and runs the Postgres database (with backups) that holds terms acceptances.',
		data: 'Everything in transit through the service (tokens, crate files during publishing), the in-memory caches, the service’s logs, and the terms acceptance records.',
		location: 'United States, US East (Virginia) region',
		url: 'https://railway.com/legal/privacy'
	},
	{
		name: 'GitHub',
		purpose:
			'Identity, permissions and storage. Your crates and index live in your own organisation’s repository; GitHub Actions signs provenance.',
		data: 'Your crates, index and release history (in your repository, under your agreement with GitHub); the API calls we make on your behalf.',
		location: 'Under your organisation’s GitHub agreement',
		url: 'https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement'
	},
	{
		name: 'Stripe',
		purpose:
			'Subscriptions, invoices, card payments and the trial-ending reminder email, from general availability. Billing is off during the preview, so Stripe receives nothing yet.',
		data: 'The organisation’s GitHub name and ID, the billing email you give us, and card details (entered on Stripe; we never see them).',
		location: 'United States and elsewhere, under Stripe’s terms',
		url: 'https://stripe.com/privacy'
	},
	{
		name: 'Cloudflare',
		purpose:
			'DNS for privatecrates.dev (not proxied: traffic goes straight to Railway), and the status page at status.privatecrates.dev.',
		data: 'DNS lookups for our domains. The status page probes only our public endpoints and holds no customer data.',
		location: 'Global network',
		url: 'https://www.cloudflare.com/privacypolicy/'
	}
];
