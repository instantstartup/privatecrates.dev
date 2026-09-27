<script lang="ts">
	import type { IllustrationProps } from './types';

	interface Props extends IllustrationProps {
		/** Draw the lamp's beams. */
		beams?: boolean;
	}

	let { beams = true, title, class: klass, x, y, width, height }: Props = $props();
	const id = $props.id();
</script>

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
	</defs>

	{#if beams}
		<g class="beams">
			<path d="M100 62 0 34V96Z" />
			<path d="M100 62 200 34V96Z" />
		</g>
	{/if}

	<!-- Rocks -->
	<path class="rock" d="M40 246Q44 222 64 218 76 206 100 214 124 204 138 218 158 220 162 246Z" />

	<!-- Tower with red bands -->
	<path class="tower" d="M78 222 86 88H114L122 222Z" />
	<g clip-path="url(#{id}-tower)">
		<rect class="band" x="70" y="112" width="60" height="22" />
		<rect class="band" x="70" y="158" width="60" height="22" />
		<rect class="band" x="70" y="204" width="60" height="22" />
	</g>
	<path class="outline" d="M78 222 86 88H114L122 222Z" />
	<rect class="door" x="94" y="196" width="12" height="20" rx="6" />

	<!-- Gallery, lantern and roof -->
	<rect class="gallery" x="80" y="80" width="40" height="8" rx="1" />
	<path class="rail" d="M80 80V72H120V80M88 72v8M96 72v8M104 72v8M112 72v8" />
	<rect class="lamp" x="88" y="50" width="24" height="22" />
	<path class="rail" d="M100 50v22" />
	<path class="roof" d="M84 50 100 32 116 50Z" />
	<path class="rail" d="M100 32v-8" />
</svg>

<style>
	.lighthouse {
		overflow: visible;
	}
	.beams path {
		fill: var(--signal);
		opacity: 0.35;
	}
	.beams {
		animation: sweep 5s ease-in-out infinite;
		transform-origin: 100px 62px;
	}
	@keyframes sweep {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.45;
		}
	}
	.rock {
		fill: var(--water-2);
		stroke: var(--ink);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.tower {
		fill: var(--deck);
	}
	.band {
		fill: var(--container);
	}
	.outline,
	.gallery,
	.lamp,
	.roof,
	.door {
		stroke: var(--ink);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.outline {
		fill: none;
	}
	.gallery,
	.roof,
	.door {
		fill: var(--ink);
	}
	.lamp {
		fill: var(--signal);
	}
	.rail {
		fill: none;
		stroke: var(--ink);
		stroke-width: 1.75;
		stroke-linecap: round;
	}
</style>
