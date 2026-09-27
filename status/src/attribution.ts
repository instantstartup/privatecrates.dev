/**
 * Decides each component's state and, above all, whose problem it is. Pure: no I/O, no clock.
 *
 * The rules (docs/trust-and-status.md §1, "Attribution"), for each component:
 *
 * 1. **Our probes.** If `/healthz` fails, every component that runs on our server is in outage; if the canary registry
 *    does not answer Cargo, the components on tenant hosts are too; if `/api/status` fails (not merely missing), the
 *    website is degraded. None of these probes calls GitHub or Stripe, so a failure is ours whatever upstream says:
 *    it is listed first, and any upstream incident after it.
 * 2. **Upstream reports a problem** with a dependency (the provider's Statuspage component is not operational):
 *    - a dependency we never call ourselves (GitHub Actions, where customers publish from): the component is
 *      degraded (outage for a major outage), attributed upstream, with a link to the provider's incident;
 *    - a dependency we call, and the server's own error rate for that provider is high: degraded or outage by our
 *      measured error rate, attributed upstream first, with our error rate as supporting evidence;
 *    - a dependency we call, and our error rate is normal: the component stays operational with a note ("GitHub
 *      degraded but we're fine");
 *    - a dependency we call, and we cannot see the server's error rate: we take upstream's word for it.
 * 3. **Nothing reported upstream but the server's error rate is high**: "Errors calling GitHub's API" (or Stripe's),
 *    blaming neither us nor them.
 * 4. Upstream effects are capped per component (registry reads only need GitHub on cold caches: at most degraded).
 * 5. An active incident we have written raises its components to at least its impact, linked to the write-up.
 */
import {
	cap,
	COMPONENTS,
	PROVIDER_NAMES,
	worst,
	type ComponentDefinition,
	type ComponentId,
	type Provider,
	type State,
} from './model.ts';
import {
	describeErrors,
	errorLevel,
	GITHUB_THRESHOLDS,
	STRIPE_THRESHOLDS,
	type CallCounts,
	type ServerStatus,
} from './server-status.ts';
import {
	componentStatus,
	describeUpstreamStatus,
	incidentFor,
	upstreamState,
	type UpstreamIncident,
	type UpstreamSnapshot,
	type UpstreamStatus,
} from './statuspage.ts';

export interface ProbeResult {
	ok: boolean;
	/** HTTP status, or 0 when there was no response. */
	status: number;
	ms: number;
	/** Why it failed: `timeout`, `network error`, `HTTP 502`, `no WWW-Authenticate header`, … */
	error?: string;
}

export interface Observation {
	checkedAt: string;
	healthz: ProbeResult;
	apiStatus: ProbeResult;
	/** `/api/status` answered but is not there (404, or not the JSON we expect): the server predates it. */
	apiStatusMissing: boolean;
	canary: ProbeResult;
	server?: ServerStatus;
}

export type Upstreams = Partial<Record<Provider, UpstreamSnapshot>>;

/** One of our incidents that is active now. */
export interface ActiveIncident {
	id: string;
	title: string;
	components: ComponentId[];
	impact: Exclude<State, 'operational'>;
	href: string;
}

export type ReasonKind = 'ours' | 'upstream' | 'api-errors' | 'incident' | 'note';

export interface Reason {
	kind: ReasonKind;
	text: string;
	detail?: string;
	href?: string;
	/** The link's text, e.g. the upstream incident's title. */
	linkText?: string;
}

export type Cause = 'none' | 'ours' | 'upstream' | 'api-errors' | 'incident';

export interface ComponentStatus {
	id: ComponentId;
	name: string;
	state: State;
	cause: Cause;
	/** Why it is not operational, most relevant first. */
	reasons: Reason[];
	/** Informational only: upstream trouble that is not reaching us. */
	notes: Reason[];
}

export interface Evaluation {
	checkedAt: string;
	state: State;
	components: ComponentStatus[];
}

const THRESHOLDS = { github: GITHUB_THRESHOLDS, stripe: STRIPE_THRESHOLDS };

function describeProbe(p: ProbeResult): string {
	return p.error ?? (p.status ? `HTTP ${p.status}` : 'no response');
}

function ourReasons(def: ComponentDefinition, obs: Observation): { state: State; reasons: Reason[] } {
	if (def.server && !obs.healthz.ok) {
		return {
			state: 'outage',
			reasons: [
				{
					kind: 'ours',
					text: obs.healthz.status === 0 ? 'Our server is not responding' : 'Our server is failing its health check',
					detail: `Health check: ${describeProbe(obs.healthz)}`,
				},
			],
		};
	}
	const reasons: Reason[] = [];
	let state: State = 'operational';
	if (def.registryHost && !obs.canary.ok) {
		state = 'outage';
		reasons.push({
			kind: 'ours',
			text: 'The registry is not answering Cargo',
			detail: `Canary registry: ${describeProbe(obs.canary)}`,
		});
	}
	if (def.statusEndpoint && !obs.apiStatus.ok && !obs.apiStatusMissing) {
		state = worst(state, 'degraded');
		reasons.push({
			kind: 'ours',
			text: 'Our server is answering with errors',
			detail: `Status endpoint: ${describeProbe(obs.apiStatus)}`,
		});
	}
	return { state, reasons };
}

function upstreamReason(provider: Provider, snap: UpstreamSnapshot, dep: string, status: UpstreamStatus): Reason {
	const who = dep === '*' ? PROVIDER_NAMES[provider] : `${PROVIDER_NAMES[provider]} ${dep}`;
	const incident: UpstreamIncident | undefined = incidentFor(snap, dep);
	if (incident) {
		return { kind: 'upstream', text: `${who} is having an incident`, href: incident.url, linkText: incident.name };
	}
	return {
		kind: 'upstream',
		text: `${who} reports ${describeUpstreamStatus(status)}`,
		href: snap.pageUrl,
		linkText: `${PROVIDER_NAMES[provider]} status`,
	};
}

function providerCounts(provider: Provider, server: ServerStatus | undefined): CallCounts | undefined {
	return provider === 'github' ? server?.github : server?.stripe;
}

function providerEffect(
	def: ComponentDefinition,
	obs: Observation,
	upstreams: Upstreams,
): { state: State; cause: Cause; reasons: Reason[]; notes: Reason[] } {
	const none = { state: 'operational' as State, cause: 'none' as Cause, reasons: [], notes: [] };
	const provider = def.provider;
	if (!provider) return none;
	const name = PROVIDER_NAMES[provider];
	const snap = upstreams[provider];
	const counts = providerCounts(provider, obs.server);
	// Undefined: we cannot see the server's own calls (no /api/status, or the provider is not configured there).
	const level = counts ? errorLevel(counts, THRESHOLDS[provider]) : undefined;

	let state: State = 'operational';
	const reasons: Reason[] = [];
	const notes: Reason[] = [];
	let observedImpaired = false;

	if (snap?.available) {
		for (const dep of def.dependencies) {
			const status = componentStatus(snap, dep.name);
			if (!status || status === 'operational') continue;
			const reason = upstreamReason(provider, snap, dep.name, status);
			if (!dep.observed || level === undefined) {
				state = worst(state, upstreamState(status));
				reasons.push(reason);
			} else if (level === 'none') {
				notes.push({ ...reason, kind: 'note', text: `${reason.text}; we are not seeing errors calling it` });
			} else {
				observedImpaired = true;
				state = worst(state, level);
				reasons.push(reason);
			}
		}
	}

	if (reasons.length > 0) {
		if (observedImpaired && counts) {
			reasons.push({
				kind: 'api-errors',
				text: `We are also seeing errors calling ${name}'s API`,
				detail: describeErrors(counts),
			});
		}
		return { state: cap(state, def.upstreamCap), cause: 'upstream', reasons, notes };
	}
	if (counts && level && level !== 'none') {
		return {
			state: cap(level, def.upstreamCap),
			cause: 'api-errors',
			reasons: [{ kind: 'api-errors', text: `Errors calling ${name}'s API`, detail: describeErrors(counts) }],
			notes,
		};
	}
	return { ...none, notes };
}

export function evaluateComponent(
	def: ComponentDefinition,
	obs: Observation,
	upstreams: Upstreams,
	incidents: readonly ActiveIncident[] = [],
): ComponentStatus {
	const ours = ourReasons(def, obs);
	const up = providerEffect(def, obs, upstreams);

	let state = worst(ours.state, up.state);
	let cause: Cause;
	let reasons: Reason[];
	if (ours.reasons.length > 0) {
		cause = 'ours';
		reasons = [...ours.reasons, ...up.reasons];
	} else {
		cause = up.cause;
		reasons = up.reasons;
	}

	for (const incident of incidents) {
		if (!incident.components.includes(def.id)) continue;
		state = worst(state, incident.impact);
		if (cause === 'none') cause = 'incident';
		reasons.push({ kind: 'incident', text: 'See our incident report', href: incident.href, linkText: incident.title });
	}

	return { id: def.id, name: def.name, state, cause, reasons, notes: up.notes };
}

export function evaluate(
	obs: Observation,
	upstreams: Upstreams,
	incidents: readonly ActiveIncident[] = [],
): Evaluation {
	const components = COMPONENTS.map((def) => evaluateComponent(def, obs, upstreams, incidents));
	return { checkedAt: obs.checkedAt, state: worst(...components.map((c) => c.state)), components };
}

export function headline(state: State): string {
	switch (state) {
		case 'operational':
			return 'All systems operational';
		case 'degraded':
			return 'Some systems are degraded';
		case 'outage':
			return 'Some systems are down';
	}
}

export function stateLabel(state: State): string {
	switch (state) {
		case 'operational':
			return 'Operational';
		case 'degraded':
			return 'Degraded';
		case 'outage':
			return 'Outage';
	}
}
