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

		<!-- Water -->
		<rect class="water" y="318" width="720" height="122" />
		<path class="glint" d="M356 336h88M372 352h56M384 368h32" />

		<!-- The lighthouse on its island, drawn over the water so the island sits on the sea -->
		<path
			class="island"
			d="M566 346C570 336 580 330 592 331L600 324C612 318 628 320 636 315C650 310 664 316 672 320C684 319 696 325 702 333C708 337 711 342 712 346Z"
		/>
		<path class="turf" d="M582 331C588 325 598 322 606 324C598 327 590 329 582 331Z" />
		<path class="turf" d="M670 321C682 319 694 324 700 331C690 327 680 324 670 321Z" />
		<path class="surf" d="M562 350h22M594 354h36M672 350h40M580 360q8-3 16 0q8 3 16 0" />
		<Lighthouse x={560} y={137} width={160} height={200} />

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
	.island {
		fill: var(--rock);
		stroke: var(--ink);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.turf {
		fill: var(--turf);
	}
	.surf {
		fill: none;
		stroke: var(--foam);
		stroke-width: 2.5;
		stroke-linecap: round;
		opacity: 0.85;
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
