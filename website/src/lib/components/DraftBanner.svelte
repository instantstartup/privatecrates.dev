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
		border: 2px dashed var(--danger-strong);
		background: var(--danger-bg);
		border-radius: 8px;
		padding: 1rem 1.1rem;
	}
	.title {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		line-height: 1.1;
		margin-bottom: 0.35rem;
	}
</style>
