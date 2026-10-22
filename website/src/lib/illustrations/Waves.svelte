<script lang="ts">
	import type { IllustrationProps } from './types';

	let { class: klass, x, y, width, height }: IllustrationProps = $props();

	// One wave period is 80 units; the paths run one period past each edge so the drift loops seamlessly.
	function wave(baseline: number, amp: number): string {
		let d = `M-80 ${baseline}`;
		for (let x = -80; x < 1280; x += 80) {
			d += ` q20 ${-amp} 40 0 t40 0`;
		}
		return d;
	}
	const back = wave(12, 6);
	const front = wave(26, 7);
</script>

<svg
	viewBox="0 0 1200 40"
	preserveAspectRatio="none"
	class={['waves', klass]}
	{x}
	{y}
	{width}
	{height}
	aria-hidden="true"
	focusable="false"
>
	<path class="back" d="{back} V40 H-80Z" />
	<path class="front" d="{front} V40 H-80Z" />
</svg>

<style>
	.waves {
		display: block;
		overflow: hidden;
	}
	.back {
		fill: var(--scene-sea-light);
		animation: drift 14s linear infinite;
	}
	.front {
		fill: var(--scene-sea);
		animation: drift 9s linear infinite reverse;
	}
	/* Drawing geometry in viewBox units (SVG's default transform-box, view-box), not a CSS length: one wave period. */
	@keyframes drift {
		from {
			transform: translateX(0);
		}
		to {
			transform: translateX(80px);
		}
	}
</style>
