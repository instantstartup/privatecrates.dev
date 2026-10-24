<script lang="ts">
	import { COMPANY, OPERATOR } from '$lib/site';

	// The version, date and operator at the top of a legal page.
	interface Props {
		version: string;
		date: string;
		dateLabel?: string;
	}

	let { version, date, dateLabel = 'Last updated' }: Props = $props();
</script>

<dl class="meta">
	<div>
		<dt>Version</dt>
		<dd><code>{version}</code></dd>
	</div>
	<div>
		<dt>{dateLabel}</dt>
		<dd>{date}</dd>
	</div>
	<div>
		<dt>Operator</dt>
		<dd>{OPERATOR}, company number {COMPANY.number}</dd>
	</div>
</dl>

<style>
	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3) var(--space-9);
		font-size: var(--text-meta);
	}
	.meta dt {
		color: var(--ink-muted);
	}
	.meta dd {
		font-weight: 700;
	}
</style>
