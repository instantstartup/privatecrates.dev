<script lang="ts">
	import CargoShip from './CargoShip.svelte';
	import Crane from './Crane.svelte';
	import type { IllustrationProps } from './types';

	interface Props extends IllustrationProps {
		/** Stencilled on the crate being loaded; keep it short (it is 64 units wide). */
		label?: string;
	}

	let { label, title, class: klass, x, y, width, height }: Props = $props();
</script>

<!-- The crane on the quay lowering one more crate onto a ship at berth: a registry ready to take cargo. -->
<svg
	viewBox="0 -40 520 290"
	class={['dock', klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<rect class="water" x="0" y="222" width="520" height="28" />
	<path class="swell" d="M8 234q10-5 20 0t20 0M150 242q10-5 20 0t20 0M290 236q10-5 20 0t20 0" />
	<CargoShip x={0} y={90} width={400} height={160} />
	<rect class="quay" x="398" y="205" width="122" height="45" />
	<path class="bollard" d="M404 205v-7h8v7" />
	<Crane x={300} y={-40} width={200} height={250} {label} />
</svg>

<style>
	.dock {
		overflow: visible;
	}
	.water {
		fill: var(--illustration-fill);
	}
	.swell {
		fill: none;
		stroke: var(--illustration-fill-muted);
		stroke-width: 2;
		stroke-linecap: round;
	}
	.quay {
		fill: var(--divider);
		stroke: var(--line);
		stroke-width: 2;
	}
	.bollard {
		fill: var(--line);
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
</style>
