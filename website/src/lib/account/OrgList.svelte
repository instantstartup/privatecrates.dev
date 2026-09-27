<script lang="ts">
	import type { Org } from '$lib/api';
	import { memberCount, planBadge, setupLabel } from './format';

	interface Props {
		orgs: Org[];
		selected: string | null;
		/** Whether an organisation's registry works (decided by the account page). */
		isLive: (org: Org) => boolean;
	}

	let { orgs, selected, isLive }: Props = $props();
</script>

<nav aria-label="Your organisations">
	<h2 class="title">Organisations</h2>
	<ul>
		{#each orgs as org (org.id)}
			{@const badge = planBadge(org)}
			{@const setup = setupLabel(org, isLive(org))}
			{@const members = memberCount(org)}
			<li class={['row', org.login === selected && 'selected']}>
				<span class="monogram" aria-hidden="true">{org.login.slice(0, 1).toUpperCase()}</span>
				<span class="text">
					<a
						href="?org={encodeURIComponent(org.login)}"
						aria-current={org.login === selected ? 'true' : undefined}>{org.login}</a
					>
					<span class="meta">{org.role === 'admin' ? 'Admin' : 'Member'}{members ? `, ${members}` : ''}</span>
					<span class="badges">
						<span class={['badge', setup === 'Live' ? 'live' : 'idle']}>{setup}</span>
						<span class={['badge', badge.tone]}><span class="visually-hidden">Plan: </span>{badge.label}</span
						>
					</span>
				</span>
			</li>
		{/each}
	</ul>
</nav>

<style>
	.title {
		font-size: var(--text-xl);
		margin-bottom: 0.5rem;
	}
	ul {
		display: grid;
		gap: 0.35rem;
	}
	/* The whole row is the target: the link stretches over it. */
	.row {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
		padding: 0.6rem 0.75rem;
		border: 2px solid transparent;
		border-radius: 8px;
	}
	.row:hover {
		background: color-mix(in srgb, var(--ink) 6%, transparent);
	}
	.row.selected {
		border-color: var(--ink);
		background: var(--deck);
	}
	a {
		color: var(--ink);
		font-weight: 700;
		text-decoration: none;
		overflow-wrap: anywhere;
	}
	a::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: 6px;
	}
	.monogram {
		flex: none;
		display: grid;
		place-items: center;
		width: 2.1rem;
		height: 2.1rem;
		border: 2px solid var(--ink);
		border-radius: 4px;
		background: var(--signal);
		color: var(--signal-ink);
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-lg);
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		line-height: 1.3;
	}
	.meta {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		margin-top: 0.3rem;
	}
	.badge {
		flex: none;
		font-size: var(--text-sm);
		font-weight: 700;
		padding: 0.15rem 0.5rem;
		border-radius: 999px;
		border: 1.5px solid currentColor;
		white-space: nowrap;
	}
	.badge.ok {
		color: var(--ok);
		background: var(--ok-bg);
	}
	.badge.warn {
		color: var(--ink);
		background: var(--warn-bg);
		border-color: var(--signal);
	}
	.badge.danger {
		color: var(--danger);
		background: var(--danger-bg);
	}
	.badge.live {
		color: var(--signal-ink);
		background: var(--signal);
		border-color: var(--ink);
	}
	.badge.idle {
		color: var(--ink-soft);
	}
</style>
