// Facts shared by the trust centre, the security policy and the DPA template, so they cannot drift apart.

/** Where security reports go (also in static/.well-known/security.txt). */
export const SECURITY_EMAIL = 'security@privatecrates.dev';
/** Everything that is not a security report: terms, privacy, questions. */
export const CONTACT_EMAIL = 'contact@privatecrates.dev';
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
			'Hosts the PrivateCrates service: it runs the server, terminates TLS for our domains, keeps its logs, and runs the Postgres database (with backups) that holds terms acceptances and fingerprints of verified provenance.',
		data: 'Everything in transit through the service (tokens, crate files during publishing), the in-memory caches, the service’s logs, the terms acceptance records, and the provenance fingerprints.',
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
			'DNS for privatecrates.dev (not proxied: traffic goes straight to Railway), the status page at status.privatecrates.dev, and forwarding email sent to our addresses (contact@, security@).',
		data: 'DNS lookups for our domains. The status page probes only our public endpoints and holds no customer data. Email you send us passes through Cloudflare Email Routing on its way to our inbox.',
		location: 'Global network',
		url: 'https://www.cloudflare.com/privacypolicy/'
	},
	{
		name: 'Grafana Labs',
		purpose:
			'Grafana Cloud keeps the service’s logs, traces and metrics, so we can see how it is running and investigate problems.',
		data: 'Service logs (GitHub logins, organisation and crate names, error codes; never tokens), and traces and metrics that record the kind of each request, its timing and status, never which crate or organisation it named.',
		location: 'EU',
		url: 'https://grafana.com/legal/privacy-policy/'
	}
];
