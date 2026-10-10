<script lang="ts">
	import Crate from './Crate.svelte';
	import type { IllustrationProps } from './types';

	interface Props extends IllustrationProps {
		/** Stencilled on the crate hanging from the spreader. */
		label?: string;
	}

	let { label, title, class: klass, x, y, width, height }: Props = $props();
</script>

<!-- A ship-to-shore gantry crane lifting one crate. -->
<svg
	viewBox="0 0 200 250"
	class={['crane', klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<!-- Backstays from the apex to both ends of the boom -->
	<path class="stay" d="M140 22 8 76M140 22 196 76" />
	<!-- A-frame -->
	<path class="frame" d="M116 76 140 22 164 76" />
	<!-- Legs and bracing -->
	<path class="frame" d="M116 76v168M164 76v168M116 128h48M116 180h48M116 128l48 52M164 128l-48 52" />
	<path class="rail" d="M100 245h80" />
	<!-- Boom -->
	<rect class="beam" x="6" y="76" width="192" height="10" rx="1" />
	<path class="rib" d="M26 76v10M46 76v10M66 76v10M86 76v10M106 76v10M176 76v10" />
	<!-- Machinery house and cab -->
	<rect class="house" x="146" y="60" width="44" height="16" rx="1" />
	<rect class="cab" x="96" y="86" width="16" height="12" rx="1" />
	<!-- Trolley, ropes and spreader -->
	<g>
		<rect class="trolley" x="40" y="86" width="22" height="7" rx="1" />
		<path class="rope" d="M45 93v58M57 93v58" />
		<rect class="spreader" x="31" y="151" width="40" height="5" rx="1" />
		<Crate x={19} y={156} width={64} height={48} tone="accent" {label} />
	</g>
</svg>

<style>
	.crane {
		overflow: visible;
	}
	.frame,
	.stay,
	.rope,
	.rail,
	.rib {
		fill: none;
		stroke: var(--line);
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.frame {
		stroke-width: 5;
	}
	.stay {
		stroke-width: 1.5;
	}
	.rope {
		stroke-width: 1.25;
	}
	.rail {
		stroke-width: 4;
	}
	.rib {
		stroke-width: 1.5;
	}
	.beam,
	.house,
	.cab,
	.trolley,
	.spreader {
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.beam {
		fill: var(--action);
	}
	.house,
	.cab {
		fill: var(--surface-raised);
	}
	.trolley,
	.spreader {
		fill: var(--line);
	}
</style>
