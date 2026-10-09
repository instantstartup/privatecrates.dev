<script lang="ts">
	import type { Org } from '$lib/api';
	import { memberCount, planBadge, plural, setupLabel } from './format';

	interface Props {
		orgs: Org[];
		selected: string | null;
		/** Whether an organisation's registry works (decided by the account page). */
		isLive: (org: Org) => boolean;
		/** The preview: every organisation is free, so no plan badges. */
		preview?: boolean;
	}

	let { orgs, selected, isLive, preview = false }: Props = $props();
</script>

<nav aria-label="Your accounts">
	<h2 class="title">{orgs.some((o) => o.personal) ? 'Accounts' : 'Organisations'}</h2>
	{#if preview}
		<!-- One line for all of them, in place of a plan badge on each. -->
		<p class="preview">Free during the preview</p>
	{/if}
	<ul>
		{#each orgs as org (org.id)}
			{@const badge = preview ? null : planBadge(org)}
			{@const setup = setupLabel(org, isLive(org))}
			{@const members = preview && org.members != null ? plural(org.members, 'member') : memberCount(org)}
			<li class={['row', org.login === selected && 'selected']}>
				<span class="monogram" aria-hidden="true">{org.login.slice(0, 1).toUpperCase()}</span>
				<span class="text">
					<a
						href="?org={encodeURIComponent(org.login)}"
						aria-current={org.login === selected ? 'true' : undefined}>{org.login}</a
					>
					<span class="meta"
						>{#if org.personal}Your personal account, always free{:else}{org.role === 'admin'
								? 'Admin'
								: 'Member'}{members ? `, ${members}` : ''}{/if}</span
					>
					<span class="badges">
						<span class={['badge', setup === 'Live' ? 'live' : 'idle']}>{setup}</span>
						{#if badge}
							<span class={['badge', badge.tone]}
								><span class="visually-hidden">Plan: </span>{badge.label}</span
							>
						{/if}
					</span>
				</span>
			</li>
		{/each}
	</ul>
</nav>

<style>
	.title {
		font-size: var(--text-title);
		margin-bottom: var(--space-3);
	}
	.preview {
		display: inline-block;
		margin-bottom: var(--space-3);
		font-size: var(--text-meta);
		font-weight: 700;
		padding: var(--space-badge-y) var(--space-3);
		border-radius: var(--radius-pill);
		border: var(--border-badge) solid currentColor;
		color: var(--ok);
		background: var(--ok-bg);
	}
	ul {
		display: grid;
		gap: var(--space-list-item);
	}
	/* The whole row is the target: the link stretches over it. */
	.row {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: var(--space-4);
		padding: var(--space-row-y) var(--space-4);
		border: var(--border-outline) solid transparent;
		border-radius: var(--radius-card);
	}
	.row:hover {
		background: color-mix(in srgb, var(--ink) 6%, transparent);
	}
	.row.selected {
		border-color: var(--ink);
		background: var(--surface-raised);
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
		border-radius: var(--radius-control);
	}
	.monogram {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--size-marker);
		height: var(--size-marker);
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-inline);
		background: var(--action);
		color: var(--action-ink);
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-lead);
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		line-height: 1.3;
	}
	.meta {
		font-size: var(--text-meta);
		color: var(--ink-muted);
	}
	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-compact);
		margin-top: var(--space-compact);
	}
	.badge {
		flex: none;
		font-size: var(--text-meta);
		font-weight: 700;
		padding: var(--space-badge-y) var(--space-3);
		border-radius: var(--radius-pill);
		border: var(--border-badge) solid currentColor;
		white-space: nowrap;
	}
	.badge.ok {
		color: var(--ok);
		background: var(--ok-bg);
	}
	.badge.warn {
		color: var(--ink);
		background: var(--warn-bg);
		border-color: var(--action);
	}
	.badge.danger {
		color: var(--danger);
		background: var(--danger-bg);
	}
	.badge.live {
		color: var(--action-ink);
		background: var(--action);
		border-color: var(--ink);
	}
	.badge.idle {
		color: var(--ink-muted);
	}
</style>
