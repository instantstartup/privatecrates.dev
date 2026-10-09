<script lang="ts">
	import { TERMS_PATH } from '$lib/site';
</script>

<div class="draft" role="note">
	<p class="title">Draft: not yet in force during the preview</p>
	<p>
		This page shows the intended structure and has not been reviewed by a lawyer. It is not an agreement, and
		highlighted items are not decided yet. During the preview, the <a href={TERMS_PATH}>preview terms</a>
		apply, and the <a href="/trust">trust centre</a> describes how the service handles data today.
	</p>
</div>

<style>
	.draft {
		border: var(--border-outline) dashed var(--danger-strong);
		background: var(--danger-bg);
		border-radius: var(--radius-card);
		padding: var(--space-5) var(--space-control-x);
	}
	.title {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-title);
		line-height: 1.1;
		margin-bottom: var(--space-2);
	}
</style>
