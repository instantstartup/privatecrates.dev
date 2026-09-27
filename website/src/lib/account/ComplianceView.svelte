<script lang="ts">
	import { untrack } from 'svelte';
	import {
		api,
		ApiError,
		auditCsvUrl,
		type AuditEntry,
		type Compliance,
		type ComplianceProblem,
		type Risk
	} from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import ErrorNotice from './ErrorNotice.svelte';
	import { formatDateTime, plural } from './format';

	interface Props {
		org: string;
		/** The registry name, for the fixes that mention Cargo.toml. */
		slug: string;
	}

	let { org, slug }: Props = $props();

	let data = $state<Compliance | null>(null);
	let error = $state<ApiError | null>(null);
	let loading = $state(false);

	// The audit log pages back with ?before=; `cursors` holds the value for each page shown so far ('' = newest).
	let audit = $state<AuditEntry[]>([]);
	let cursors = $state<string[]>(['']);
	let auditBusy = $state(false);
	let auditError = $state<ApiError | null>(null);
	let noOlder = $state(false);

	function toApiError(e: unknown): ApiError {
		return e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
	}

	async function load() {
		loading = true;
		error = null;
		try {
			const doc = await api.compliance(org);
			data = doc;
			audit = doc.audit;
			cursors = [''];
			noOlder = doc.audit.length === 0 || doc.audit_next_before === null;
		} catch (e) {
			error = toApiError(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void org;
		untrack(load);
	});

	/** The `before` value for the page after the one shown. */
	function nextCursor(): string | null {
		if (!data || audit.length === 0) return null;
		if (cursors.length === 1 && data.audit_next_before !== undefined) return data.audit_next_before;
		return audit[audit.length - 1].at;
	}

	let auditHeading = $state<HTMLElement | null>(null);

	async function showPage(before: string, direction: 'older' | 'newer') {
		auditBusy = true;
		auditError = null;
		try {
			const doc = await api.compliance(org, before || undefined);
			if (direction === 'older' && doc.audit.length === 0) {
				noOlder = true;
				return;
			}
			audit = doc.audit;
			cursors = direction === 'older' ? [...cursors, before] : cursors.slice(0, -1);
			noOlder = doc.audit_next_before === null;
			auditHeading?.scrollIntoView({ block: 'start' });
		} catch (e) {
			auditError = toApiError(e);
		} finally {
			auditBusy = false;
		}
	}

	function older() {
		const before = nextCursor();
		if (before) void showPage(before, 'older');
	}

	function newer() {
		if (cursors.length > 1) void showPage(cursors[cursors.length - 2], 'newer');
	}

	/** Versions that passed every check: all versions less those with a problem. */
	function verifiedCount(c: Compliance): number {
		const i = c.integrity;
		const subjects = new Set(
			i.problems
				.map((p) => p.subject ?? (p.crate ? `${p.crate}@${p.version ?? ''}` : null))
				.filter((s): s is string => !!s)
		);
		if (subjects.size) return Math.max(0, i.versions - subjects.size);
		return Math.min(i.immutable, i.digest_matches, i.provenance + i.manual);
	}

	function problemTitle(p: ComplianceProblem): string {
		return p.subject ?? ([p.crate, p.version].filter(Boolean).join(' ') || 'Registry');
	}

	function problemText(p: ComplianceProblem): string {
		return p.message ?? p.detail ?? p.code ?? '';
	}

	interface RiskCopy {
		title: string;
		why: string;
		fix: string;
		link?: [string, string];
	}

	function riskCopy(r: Risk): RiskCopy {
		const crate = r.crate ?? 'A crate';
		switch (r.code) {
			case 'name_clash':
				return {
					title: `${crate} also exists on crates.io`,
					why: `A dependency on it that forgets registry = "${slug}" gets the public crate instead: a dependency-confusion risk.`,
					fix: 'Rename the crate, or check that every dependency on it names the registry. Reserve names that matter on crates.io with a placeholder crate.'
				};
			case 'manual_publish_allowed':
				return {
					title: `${crate} allows manual publishing`,
					why: 'Versions published from a developer’s machine carry no provenance, so a forged version would look like just another manual publish.',
					fix: `Set allow_manual_publish = false in owners/${r.crate ?? '<crate>'}.toml in your storage repository, and publish from CI.`,
					link: ['/docs/publishing', 'Publishing from CI']
				};
			case 'missing_provenance':
				return {
					title: `${crate} has versions without provenance`,
					why: 'Nothing signed by GitHub says which workflow built them.',
					fix: 'Review the versions listed under Integrity. Published versions cannot change, so publish the next ones from CI.',
					link: ['/docs/ci', 'CI without secrets']
				};
			case 'no_verify_workflow':
				return {
					title: 'No verifier runs in your storage repository',
					why: 'No workflow there runs privatecrates-verify, so nothing independent of us checks what we write.',
					fix: 'Add the verifier workflow to the storage repository and run it every hour.',
					link: ['/docs/verify', 'Set up the verifier']
				};
			default:
				return {
					title: r.crate ? `${r.crate}: ${r.code}` : r.code,
					why: r.detail ?? '',
					fix: ''
				};
		}
	}

	const actionLabels: Record<string, string> = {
		publish: 'Published',
		yank: 'Yanked',
		unyank: 'Unyanked',
		owners_change: 'Owners changed',
		settings_change: 'Settings changed',
		index_change: 'Index edited by a person'
	};
</script>

<section class="compliance" aria-labelledby="compliance-{org}" aria-busy={loading}>
	<div class="head">
		<h3 id="compliance-{org}">Compliance</h3>
		{#if data}
			<p class="checked">Checked {formatDateTime(data.generated_at)} UTC</p>
		{/if}
	</div>

	<Callout title="Your verifier is the independent check">
		<p>
			We run the verifier’s checks against your storage repository and show the results here. They are our own
			report on our own writes; the open-source verifier, run in your CI with your token, checks them without
			trusting us. <a href="/docs/verify">Set up the verifier</a>.
		</p>
	</Callout>

	{#if error}
		<ErrorNotice {error} title="The compliance report could not be loaded">
			<p><button class="btn btn-quiet" type="button" onclick={load} disabled={loading}>Try again</button></p>
		</ErrorNotice>
	{:else if !data}
		<div class="loading" role="status">
			<span class="bar"></span>
			<span class="bar short"></span>
			<span class="bar"></span>
			<span class="visually-hidden">Loading the compliance report…</span>
		</div>
	{:else}
		{@const i = data.integrity}
		{@const verified = verifiedCount(data)}
		{@const clean = i.problems.length === 0 && verified === i.versions}

		<section class="block" aria-labelledby="integrity-{org}">
			<h4 id="integrity-{org}">Integrity</h4>
			{#if i.versions === 0}
				<p class="empty">
					Nothing published yet. Each version is checked here once it is published: its release, its checksum
					and its provenance. <a href="/docs/publishing">Publish a first crate</a>.
				</p>
			{:else}
				<p class={['summary', clean ? 'ok' : 'bad']}>
					<span class="figure">{verified} of {i.versions}</span>
					versions verified
				</p>
				<p class="sub">
					{i.provenance} with provenance{i.manual
						? `; ${plural(i.manual, 'manual publish', 'manual publishes')}, listed for review`
						: ''}.
				</p>
				<dl class="checks">
					<div class={i.immutable === i.versions ? 'pass' : 'fail'}>
						<dt>Release present and immutable</dt>
						<dd>{i.immutable} of {i.versions}</dd>
					</div>
					<div class={i.digest_matches === i.versions ? 'pass' : 'fail'}>
						<dt>Checksum matches the index</dt>
						<dd>{i.digest_matches} of {i.versions}</dd>
					</div>
					<div class={i.provenance + i.manual === i.versions ? 'pass' : 'fail'}>
						<dt>Provenance signed by GitHub</dt>
						<dd>
							{i.provenance} of {i.versions}{i.manual ? ` (and ${i.manual} manual)` : ''}
						</dd>
					</div>
				</dl>
				{#if i.problems.length}
					<div class="problems">
						<p class="problems-title">{plural(i.problems.length, 'problem')} to look at</p>
						<ul>
							{#each i.problems as p, n (n)}
								<li class={p.severity === 'error' ? 'error' : 'warning'}>
									<code>{problemTitle(p)}</code>
									{problemText(p)}
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			{/if}
		</section>

		<section class="block" aria-labelledby="publishers-{org}">
			<h4 id="publishers-{org}">Who can publish</h4>
			{#if data.publishers.length === 0}
				<p class="empty">
					No crates yet. A crate’s first publish records its repository and workflow in
					<code>owners/</code> in your storage repository, and it appears here.
				</p>
			{:else}
				<p class="sub">From each crate’s owners file. Changing it is a commit to your storage repository.</p>
				<div class="table-scroll">
					<table>
						<thead>
							<tr>
								<th scope="col">Crate</th>
								<th scope="col">Repository</th>
								<th scope="col">Workflows</th>
								<th scope="col">Environment</th>
								<th scope="col">Manual publishing</th>
							</tr>
						</thead>
						<tbody>
							{#each data.publishers as p (p.crate)}
								<tr>
									<th scope="row"><code>{p.crate}</code></th>
									<td><a class="repo" href="https://github.com/{p.repository}">{p.repository}</a></td>
									<td>
										{#each p.workflows as w, n (w)}{#if n > 0},
											{/if}<code>{w}</code>{/each}
									</td>
									<td
										>{#if p.environment}<code>{p.environment}</code>{:else}<span class="none">Any</span
											>{/if}</td
									>
									<td>
										{#if p.manual_publish}<span class="flag warn">Allowed</span>{:else}<span class="flag"
												>Off</span
											>{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>

		<section class="block" aria-labelledby="risks-{org}">
			<h4 id="risks-{org}">Risks</h4>
			{#if data.risks.length === 0}
				<p class="empty ok-line">
					No risks found: no name clashes with crates.io, no crates that allow manual publishing, and a
					verifier workflow runs in your storage repository.
				</p>
			{:else}
				<ul class="risks">
					{#each data.risks as r, n (n)}
						{@const c = riskCopy(r)}
						<li class="risk">
							<p class="risk-title">{c.title}</p>
							{#if c.why}<p>{c.why}</p>{/if}
							{#if c.fix}<p><strong>Fix:</strong> {c.fix}</p>{/if}
							{#if c.link}<p class="more"><a href={c.link[0]}>{c.link[1]}</a></p>{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="block" aria-labelledby="audit-{org}">
			<div class="audit-head">
				<h4 id="audit-{org}" bind:this={auditHeading}>Audit log</h4>
				<a class="btn btn-quiet" href={auditCsvUrl(org)} download="{org}-audit.csv" data-sveltekit-reload
					>Download CSV</a
				>
			</div>
			<p class="sub">
				Every publish, yank and change to owners or settings, newest first, from your storage repository’s
				commit history.
			</p>
			{#if audit.length === 0}
				<p class="empty">Nothing yet. Publishes, yanks and changes to owners files appear here.</p>
			{:else}
				{#if auditError}
					<ErrorNotice error={auditError} title="That page of the audit log could not be loaded" />
				{/if}
				<div class="table-scroll">
					<table class="audit">
						<thead>
							<tr>
								<th scope="col">When (UTC)</th>
								<th scope="col">Event</th>
								<th scope="col">Crate</th>
								<th scope="col">By</th>
								<th scope="col">Provenance</th>
								<th scope="col">Commit</th>
							</tr>
						</thead>
						<tbody>
							{#each audit as e, n (e.commit ?? n)}
								<tr>
									<td class="when">{formatDateTime(e.at)}</td>
									<td>{actionLabels[e.action] ?? e.action}</td>
									<td>
										{#if e.crate}<code>{e.crate}</code>{#if e.version}&nbsp;{e.version}{/if}{:else}<span
												class="none">None</span
											>{/if}
									</td>
									<td class="by">{e.by}</td>
									<td>
										{#if e.provenance === true}Yes{:else if e.provenance === false}<span class="flag warn"
												>No</span
											>{:else}<span class="none">None</span>{/if}
									</td>
									<td
										>{#if e.commit}<code>{e.commit.slice(0, 7)}</code>{/if}</td
									>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<nav class="pager" aria-label="Audit log pages">
					<button
						class="btn btn-quiet"
						type="button"
						onclick={newer}
						disabled={auditBusy || cursors.length === 1}>Newer entries</button
					>
					<span class="page-no">Page {cursors.length}</span>
					<button class="btn btn-quiet" type="button" onclick={older} disabled={auditBusy || noOlder}>
						{auditBusy ? 'Loading…' : noOlder ? 'No older entries' : 'Older entries'}
					</button>
				</nav>
			{/if}
		</section>
	{/if}
</section>

<style>
	.compliance {
		display: grid;
		gap: 1.75rem;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.25rem 1rem;
	}
	h3 {
		font-size: var(--text-3xl);
	}
	.checked,
	.sub {
		color: var(--ink-soft);
	}
	.block {
		display: grid;
		gap: 0.75rem;
		padding-top: 1.5rem;
		border-top: 2px solid var(--line);
		min-width: 0;
	}
	h4 {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-2xl);
		line-height: 1.1;
	}
	.empty {
		color: var(--ink-soft);
		max-width: 40rem;
	}
	.ok-line::before {
		content: '✓ ';
		color: var(--ok);
		font-weight: 700;
	}

	.summary {
		font-size: var(--text-lg);
		font-weight: 700;
	}
	.figure {
		font-family: var(--font-display);
		font-size: var(--text-3xl);
		line-height: 1;
		margin-right: 0.25rem;
	}
	.summary.ok .figure {
		color: var(--ok);
	}
	.summary.bad .figure {
		color: var(--danger);
	}
	.checks {
		display: grid;
		gap: 0.35rem;
		max-width: 34rem;
	}
	.checks > div {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.4rem 0 0.4rem 1.6rem;
		border-bottom: 1px dashed var(--rule);
		position: relative;
	}
	.checks > div::before {
		position: absolute;
		left: 0;
		font-weight: 700;
	}
	.checks .pass::before {
		content: '✓';
		color: var(--ok);
	}
	.checks .fail::before {
		content: '!';
		color: var(--danger);
	}
	.checks dd {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.problems {
		border-left: 5px solid var(--danger);
		background: var(--danger-bg);
		border-radius: 0 8px 8px 0;
		padding: 0.85rem 1rem;
	}
	.problems-title,
	.risk-title {
		font-weight: 700;
	}
	.problems ul {
		display: grid;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.problems li code {
		font-weight: 700;
		margin-right: 0.35rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-sm);
	}
	th,
	td {
		text-align: left;
		vertical-align: top;
		padding: 0.55rem 0.6rem;
		border-bottom: 1px solid var(--rule);
	}
	thead th {
		white-space: nowrap;
	}
	/* Crate names, repositories and workflow files stay whole; the table scrolls sideways instead. */
	td code,
	th code,
	.repo {
		white-space: nowrap;
		overflow-wrap: normal;
	}
	.none {
		color: var(--ink-soft);
	}
	.flag {
		font-weight: 700;
	}
	.flag.warn {
		color: var(--danger);
	}
	.when {
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}
	.by {
		overflow-wrap: anywhere;
		min-width: 14rem;
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}

	.risks {
		display: grid;
		gap: 0.75rem;
	}
	.risk {
		display: grid;
		gap: 0.3rem;
		padding: 0.85rem 1rem;
		border-left: 5px solid var(--signal);
		background: var(--warn-bg);
		border-radius: 0 8px 8px 0;
		max-width: 44rem;
	}
	.more {
		font-size: var(--text-sm);
	}

	.audit-head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: 0.5rem 1rem;
	}
	.pager {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}
	.page-no {
		color: var(--ink-soft);
		font-size: var(--text-sm);
	}

	.loading {
		display: grid;
		gap: 0.75rem;
		min-height: 14rem;
		align-content: start;
	}
	.bar {
		display: block;
		height: 1.1rem;
		border-radius: 4px;
		background: color-mix(in srgb, var(--ink) 10%, transparent);
		animation: pulse 1.4s ease-in-out infinite;
	}
	.bar.short {
		width: 60%;
	}
	@keyframes pulse {
		50% {
			opacity: 0.45;
		}
	}
</style>
