/**
 * The components shown on the status page and what each one depends on (docs/trust-and-status.md §1).
 */

export type State = 'operational' | 'degraded' | 'outage';

export const STATES: readonly State[] = ['operational', 'degraded', 'outage'];

export type ComponentId = 'reads' | 'downloads' | 'publishing' | 'signin' | 'website' | 'billing';

export type Provider = 'github' | 'stripe';

export interface UpstreamDependency {
	/** The component's name on the provider's Statuspage, or `*` for the provider as a whole. */
	name: string;
	/**
	 * Whether our server calls this dependency itself, so its own error rate (from `/api/status`) shows whether the
	 * upstream problem reaches us. GitHub Actions is not observed: customers publish from it, we never call it.
	 */
	observed: boolean;
}

export interface ComponentDefinition {
	id: ComponentId;
	name: string;
	/** A short gloss shown under the name. */
	detail?: string;
	/** Down when our server's `/healthz` fails. */
	server: boolean;
	/** Down when the canary registry does not answer Cargo (tenant hosts). */
	registryHost: boolean;
	/** Degraded when the server's `/api/status` fails (it is served by the same apex as the website). */
	statusEndpoint?: boolean;
	provider?: Provider;
	dependencies: UpstreamDependency[];
	/** The worst state an upstream problem alone can cause (reads only need GitHub on cold caches). */
	upstreamCap?: State;
}

export const COMPONENTS: readonly ComponentDefinition[] = [
	{
		id: 'reads',
		name: 'Registry reads',
		detail: 'The index, and `cargo build`',
		server: true,
		registryHost: true,
		provider: 'github',
		// Webhooks revoke access at once (SPEC §7); without them, access changes wait for the caches to expire.
		// We receive them rather than call them, so our error rate says nothing about them.
		dependencies: [
			{ name: 'API Requests', observed: true },
			{ name: 'Webhooks', observed: false },
		],
		upstreamCap: 'degraded',
	},
	{
		id: 'downloads',
		name: 'Downloads',
		detail: 'Crate files, from GitHub release assets',
		server: true,
		registryHost: true,
		provider: 'github',
		dependencies: [{ name: 'API Requests', observed: true }],
	},
	{
		id: 'publishing',
		name: 'Publishing',
		detail: '`cargo publish`, usually from GitHub Actions',
		server: true,
		registryHost: true,
		provider: 'github',
		dependencies: [
			{ name: 'API Requests', observed: true },
			{ name: 'Git Operations', observed: true },
			{ name: 'Actions', observed: false },
		],
	},
	{
		id: 'signin',
		name: 'Sign-in',
		detail: 'Signing in with GitHub, and Cargo tokens',
		server: true,
		registryHost: false,
		provider: 'github',
		dependencies: [{ name: 'API Requests', observed: true }],
	},
	{
		id: 'website',
		name: 'Website and account',
		detail: 'privatecrates.dev and the account pages',
		server: true,
		registryHost: false,
		statusEndpoint: true,
		dependencies: [],
	},
	{
		id: 'billing',
		name: 'Billing',
		detail: 'Subscriptions and invoices, through Stripe',
		server: false,
		registryHost: false,
		provider: 'stripe',
		dependencies: [{ name: '*', observed: true }],
	},
];

export const COMPONENT_IDS: readonly ComponentId[] = COMPONENTS.map((c) => c.id);

export const PROVIDER_NAMES: Record<Provider, string> = { github: 'GitHub', stripe: 'Stripe' };

export function isComponentId(value: string): value is ComponentId {
	return (COMPONENT_IDS as readonly string[]).includes(value);
}

export function componentName(id: ComponentId): string {
	return COMPONENTS.find((c) => c.id === id)?.name ?? id;
}

const RANK: Record<State, number> = { operational: 0, degraded: 1, outage: 2 };

export function worst(...states: State[]): State {
	return states.reduce<State>((a, b) => (RANK[b] > RANK[a] ? b : a), 'operational');
}

export function cap(state: State, limit: State | undefined): State {
	if (!limit) return state;
	return RANK[state] > RANK[limit] ? limit : state;
}

/** One environment to probe, from the `ENVIRONMENTS` var. */
export interface Environment {
	id: string;
	name: string;
	apex: string;
	canary: string;
	/** Where its page lives: `/` or `/dev`. */
	path: string;
}
