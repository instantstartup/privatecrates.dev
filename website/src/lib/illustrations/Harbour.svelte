<script lang="ts">
	import CargoShip from './CargoShip.svelte';
	import Crane from './Crane.svelte';
	import Lighthouse from './Lighthouse.svelte';
	import Waves from './Waves.svelte';
	import type { IllustrationProps } from './types';

	let { class: klass }: IllustrationProps = $props();
	const id = $props.id();

	// Small four-point stars for the night sky: [x, y, size].
	const stars: [number, number, number][] = [
		[236, 52, 3],
		[284, 150, 2],
		[330, 38, 2.5],
		[372, 96, 2],
		[418, 58, 3.5],
		[548, 44, 2],
		[596, 104, 2.5],
		[652, 58, 3],
		[700, 124, 2]
	];
	const star = ([x, y, s]: [number, number, number]) =>
		`M${x} ${y - s * 2}Q${x} ${y} ${x + s * 2} ${y}Q${x} ${y} ${x} ${y + s * 2}Q${x} ${y} ${x - s * 2} ${y}Q${x} ${y} ${x} ${y - s * 2}Z`;
</script>

<!--
	The hero scene: a harbour at dawn in the light theme, and by moonlight, with the lighthouse lit, in the dark theme.
	The switch is pure CSS: the theme tokens --day and --night (1 or 0, set in app.css) drive the opacities below.
	Decorative; the headline beside it carries the message.
-->
<svg viewBox="0 0 720 440" class={['harbour', klass]} aria-hidden="true" focusable="false">
	<defs>
		<linearGradient id="{id}-sky" x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" class="sky-top" />
			<stop offset="1" class="sky-bottom" />
		</linearGradient>
		<radialGradient id="{id}-halo">
			<stop offset="0" class="halo-core" />
			<stop offset="1" class="halo-edge" />
		</radialGradient>
		<clipPath id="{id}-frame">
			<rect width="720" height="440" rx="14" />
		</clipPath>
	</defs>

	<g clip-path="url(#{id}-frame)">
		<rect width="720" height="440" fill="url(#{id}-sky)" />

		<!-- By day: the sun and a pair of birds -->
		<g class="by-day">
			<circle cx="470" cy="86" r="60" fill="url(#{id}-halo)" />
			<circle class="sun" cx="470" cy="86" r="26" />
			<path class="birds" d="M300 110q6-6 12 0q6-6 12 0M346 88q5-5 10 0q5-5 10 0" />
		</g>

		<!-- By night: a crescent moon and a few stars -->
		<g class="by-night">
			<circle cx="470" cy="86" r="52" fill="url(#{id}-halo)" />
			<path class="moon" d="M470 64A22 22 0 1 0 470 108A28 28 0 0 1 470 64Z" />
			<path class="stars" d={stars.map(star).join('')} />
		</g>

		<path class="headland" d="M0 318V292Q60 270 120 284T250 280Q300 290 330 318Z" />

		<!-- Water, with the light of the sun or moon on it -->
		<rect class="water" y="318" width="720" height="122" />
		<path class="glint" d="M432 334h52M444 348h28M450 362h16" />

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
		stop-color: var(--scene-sky);
	}
	.sky-bottom {
		stop-color: var(--scene-sky-low);
	}
	.by-day {
		opacity: var(--day);
	}
	.by-night {
		opacity: var(--night);
	}
	.sun {
		fill: var(--action);
		opacity: 0.9;
	}
	.moon {
		fill: var(--scene-glint);
	}
	.halo-core {
		stop-color: var(--scene-glint);
		stop-opacity: calc(0.18 + 0.22 * var(--day));
	}
	.halo-edge {
		stop-color: var(--scene-glint);
		stop-opacity: 0;
	}
	.stars {
		fill: var(--scene-glint);
		opacity: 0.8;
	}
	.headland {
		fill: var(--scene-sea-light);
		opacity: 0.55;
	}
	.birds {
		fill: none;
		stroke: var(--line);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.water {
		fill: var(--scene-sea);
	}
	.island {
		fill: var(--scene-rock);
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.turf {
		fill: var(--scene-grass);
	}
	.surf {
		fill: none;
		stroke: var(--scene-foam);
		stroke-width: 2.5;
		stroke-linecap: round;
		opacity: 0.85;
	}
	.glint {
		fill: none;
		stroke: var(--scene-glint);
		stroke-width: 3;
		stroke-linecap: round;
		opacity: calc(0.75 - 0.2 * var(--night));
	}
	.quay {
		fill: var(--scene-quay);
		stroke: var(--line);
		stroke-width: 2;
	}
	.bollard {
		fill: var(--line);
	}
	.frame {
		fill: none;
		stroke: var(--line);
		stroke-width: 2;
	}
</style>
