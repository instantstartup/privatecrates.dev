<script lang="ts">
	import type { IllustrationProps } from './types';

	let { title, class: klass, x, y, width, height }: IllustrationProps = $props();
</script>

<svg
	viewBox="0 0 48 48"
	class={['anchor', klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<circle cx="24" cy="8" r="4" />
	<path d="M24 12v30M15 20h18M7 28q2 13 17 14 15-1 17-14M4 31l3-4 4 3M44 31l-3-4-4 3" />
</svg>

<style>
	.anchor {
		fill: none;
		stroke: currentColor;
		stroke-width: 3;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
