<script lang="ts">
	import type { IllustrationProps } from './types';

	interface Props extends IllustrationProps {
		/** Draw the lamp's beams. Like the lamp, they only shine at night. */
		beams?: boolean;
	}

	let { beams = true, title, class: klass, x, y, width, height }: Props = $props();
	const id = $props.id();
</script>

<!--
	The lamp is lit only at night. The theme tokens --day and --night (1 or 0, set in app.css for the dark theme and
	inside .night bands) drive the opacities below, so the switch is pure CSS: no script, no flash.
-->
<svg
	viewBox="0 0 200 250"
	class={['lighthouse', klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<defs>
		<clipPath id="{id}-tower">
			<path d="M78 222 86 88H114L122 222Z" />
		</clipPath>
		<!-- Beams fade out as they leave the lamp. -->
		<linearGradient id="{id}-beam-left" x1="100" y1="0" x2="0" y2="0" gradientUnits="userSpaceOnUse">
			<stop offset="0" class="beam-near" />
			<stop offset="1" class="beam-far" />
		</linearGradient>
		<linearGradient id="{id}-beam-right" x1="100" y1="0" x2="200" y2="0" gradientUnits="userSpaceOnUse">
			<stop offset="0" class="beam-near" />
			<stop offset="1" class="beam-far" />
		</linearGradient>
		<radialGradient id="{id}-glow">
			<stop offset="0" class="glow-core" />
			<stop offset="1" class="glow-edge" />
		</radialGradient>
	</defs>

	{#if beams}
		<g class="beams">
			<path d="M100 61 0 36V88Z" fill="url(#{id}-beam-left)" />
			<path d="M100 61 200 36V88Z" fill="url(#{id}-beam-right)" />
		</g>
	{/if}
	<circle class="glow" cx="100" cy="61" r="34" fill="url(#{id}-glow)" />

	<!-- Rocks -->
	<path class="rock" d="M40 246Q44 222 64 218 76 206 100 214 124 204 138 218 158 220 162 246Z" />

	<!-- Tower with red bands -->
	<path class="tower" d="M78 222 86 88H114L122 222Z" />
	<g clip-path="url(#{id}-tower)">
		<rect class="band" x="70" y="112" width="60" height="22" />
		<rect class="band" x="70" y="158" width="60" height="22" />
		<rect class="band" x="70" y="204" width="60" height="22" />
	</g>
	<!-- Not `outline`: Tailwind would add its outline utility, a box round the path. -->
	<path class="tower-edge" d="M78 222 86 88H114L122 222Z" />
	<rect class="door" x="94" y="196" width="12" height="20" rx="6" />

	<!-- Gallery, lantern (glass by day, lit at night) and roof -->
	<rect class="gallery" x="80" y="80" width="40" height="8" rx="1" />
	<path class="rail" d="M80 80V72H120V80M88 72v8M96 72v8M104 72v8M112 72v8" />
	<rect class="lamp" x="88" y="50" width="24" height="22" />
	<path class="pane-glint" d="M91.5 67 96 55M104.5 67 109 55" />
	<path class="rail" d="M100 50v22" />
	<path class="roof" d="M84 50 100 32 116 50Z" />
	<path class="rail" d="M100 32v-8" />
</svg>

<style>
	.lighthouse {
		overflow: visible;
	}
	.beams,
	.glow {
		opacity: var(--night);
	}
	/* A slow, shallow breathing of the beams. The global prefers-reduced-motion rule stops it. */
	.beams path {
		animation: breathe 6s ease-in-out infinite;
	}
	@keyframes breathe {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.6;
		}
	}
	.beam-near {
		stop-color: var(--action);
		stop-opacity: 0.42;
	}
	.beam-far,
	.glow-edge {
		stop-color: var(--action);
		stop-opacity: 0;
	}
	.glow-core {
		stop-color: var(--action);
		stop-opacity: 0.5;
	}
	.rock {
		fill: var(--scene-sea-light);
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.tower {
		fill: var(--surface-raised);
	}
	.band {
		fill: var(--accent);
	}
	.tower-edge,
	.gallery,
	.lamp,
	.roof,
	.door {
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.tower-edge {
		fill: none;
	}
	.gallery,
	.roof,
	.door {
		fill: var(--line);
	}
	.lamp {
		fill: var(--scene-lamp);
	}
	/* Light catching the glass: only by day. */
	.pane-glint {
		fill: none;
		stroke: var(--scene-glare);
		stroke-width: 1.75;
		stroke-linecap: round;
		opacity: calc(0.85 * var(--day));
	}
	.rail {
		fill: none;
		stroke: var(--line);
		stroke-width: 1.75;
		stroke-linecap: round;
	}
</style>
