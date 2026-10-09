<script lang="ts">
	import Crate from './Crate.svelte';
	import Waves from './Waves.svelte';
	import type { IllustrationProps } from './types';

	let { class: klass }: IllustrationProps = $props();
</script>

<!-- A single crate adrift, with a gull keeping watch. Decorative. -->
<svg viewBox="0 0 400 220" class={['lost', klass]} aria-hidden="true" focusable="false">
	<path class="ripple" d="M110 176q30-8 60 0M232 180q30-8 60 0M150 196q50-10 100 0" />
	<g class="drift">
		<g transform="rotate(-9 200 140)">
			<Crate x={146} y={96} width={108} height={81} tone="primary" label="404" />
		</g>
		<!-- Gull on the crate -->
		<g class="gull" transform="translate(222 70) rotate(-9)">
			<path class="gull-body" d="M0 20q2-12 14-12 8 0 12 6l8 1-7 4q-3 7-14 7Q2 26 0 20Z" />
			<path class="gull-wing" d="M6 16q8-2 14 4" />
			<circle class="gull-eye" cx="22" cy="12" r="1.3" />
			<path class="gull-legs" d="M11 26v5M16 26v5" />
		</g>
	</g>
	<Waves x={0} y={180} width={400} height={40} />
</svg>

<style>
	.lost {
		display: block;
		width: 100%;
		height: auto;
		overflow: visible;
	}
	.drift {
		animation: drift 6s ease-in-out infinite;
		transform-origin: 200px 170px;
	}
	@keyframes drift {
		0%,
		100% {
			transform: translateY(0) rotate(0deg);
		}
		50% {
			transform: translateY(5px) rotate(3deg);
		}
	}
	.ripple {
		fill: none;
		stroke: var(--illustration-fill-muted);
		stroke-width: 3;
		stroke-linecap: round;
	}
	.gull-body {
		fill: var(--surface-raised);
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.gull-wing,
	.gull-legs {
		fill: none;
		stroke: var(--line);
		stroke-width: 2;
		stroke-linecap: round;
	}
	.gull-eye {
		fill: var(--line);
	}
</style>
