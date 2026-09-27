<script lang="ts">
	import CargoShip from './CargoShip.svelte';
	import Crane from './Crane.svelte';
	import Lighthouse from './Lighthouse.svelte';
	import Waves from './Waves.svelte';
	import type { IllustrationProps } from './types';

	let { class: klass }: IllustrationProps = $props();
	const id = $props.id();
</script>

<!-- The hero scene: a harbour at dawn. Decorative; the headline beside it carries the message. -->
<svg viewBox="0 0 720 440" class={['harbour', klass]} aria-hidden="true" focusable="false">
	<defs>
		<linearGradient id="{id}-sky" x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" class="sky-top" />
			<stop offset="1" class="sky-bottom" />
		</linearGradient>
		<clipPath id="{id}-frame">
			<rect width="720" height="440" rx="14" />
		</clipPath>
	</defs>

	<g clip-path="url(#{id}-frame)">
		<rect width="720" height="440" fill="url(#{id}-sky)" />

		<!-- Low sun and a far headland -->
		<circle class="sun" cx="400" cy="318" r="64" />
		<path class="headland" d="M0 318V292Q60 270 120 284T250 280Q300 290 330 318Z" />
		<path class="birds" d="M300 110q6-6 12 0q6-6 12 0M346 88q5-5 10 0q5-5 10 0" />

		<!-- Lighthouse beam across the sky, drawn behind the tower -->
		<Lighthouse x={560} y={130} width={184} height={230} />

		<!-- Water -->
		<rect class="water" y="318" width="720" height="122" />
		<path class="glint" d="M356 336h88M372 352h56M384 368h32" />

		<!-- Quay and crane -->
		<rect class="quay" x="-4" y="306" width="238" height="26" rx="2" />
		<path class="bollard" d="M200 306v-8h10v8" />
		<Crane x={-2} y={70} width={196} height={245} label="0.2.0" />

		<!-- The ship, alongside -->
		<CargoShip x={196} y={196} width={380} height={152} />

		<Waves x={0} y={372} width={720} height={70} />
	</g>
	<rect class="frame" x="1" y="1" width="718" height="438" rx="14" />
</svg>

<style>
	.harbour {
		display: block;
		width: 100%;
		height: auto;
	}
	.sky-top {
		stop-color: var(--dawn-2);
	}
	.sky-bottom {
		stop-color: var(--dawn);
	}
	.sun {
		fill: var(--signal);
		opacity: 0.9;
	}
	.headland {
		fill: var(--water-2);
		opacity: 0.55;
	}
	.birds {
		fill: none;
		stroke: var(--ink);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.water {
		fill: var(--water);
	}
	.glint {
		stroke: var(--signal);
		stroke-width: 3;
		stroke-linecap: round;
		opacity: 0.75;
	}
	.quay {
		fill: var(--ink-soft);
		stroke: var(--ink);
		stroke-width: 2;
	}
	.bollard {
		fill: var(--ink);
	}
	.frame {
		fill: none;
		stroke: var(--ink);
		stroke-width: 2;
	}
</style>
