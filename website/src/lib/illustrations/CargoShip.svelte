<script lang="ts">
	import type { IllustrationProps } from './types';

	let { title, class: klass, x, y, width, height }: IllustrationProps = $props();

	// Crate stacks on deck: [column, rows high, tone of each row from the deck up].
	const tones = ['primary', 'accent', 'muted', 'raised'] as const;
	const stacks: number[][] = [[1, 2, 0], [2, 0, 3], [0, 1], [3, 2, 1], [1, 0, 2], [2, 3], [0]];
	const W = 36;
	const H = 17;
	const DECK = 96;
</script>

<svg
	viewBox="0 0 400 160"
	class={['ship', klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<g class="bob">
		<!-- Crates on deck -->
		{#each stacks as stack, col (col)}
			{#each stack as t, row (row)}
				<g transform="translate({100 + col * (W + 2)} {DECK - (row + 1) * H})">
					<rect class="box tone-{tones[t]}" width={W} height={H} rx="1.5" />
					<path class="rib" d="M9 3v11M18 3v11M27 3v11" />
				</g>
			{/each}
		{/each}

		<!-- Bridge and funnel at the stern -->
		<rect class="cabin" x="40" y="44" width="46" height="52" rx="2" />
		<path class="window" d="M47 56h32M47 68h32" />
		<rect class="funnel" x="52" y="24" width="18" height="20" rx="1" />
		<path class="funnel-band" d="M52 31h18" />
		<path class="mast" d="M378 96V70M372 76h12" />

		<!-- Hull, with a red boot-top at the waterline -->
		<path class="hull" d="M12 96H394L374 146H46Q26 132 12 96Z" />
		<path class="boot" d="M33 130H380.6L374 146H46Q38 140 33 130Z" />
		<circle class="porthole" cx="344" cy="112" r="3" />
		<circle class="porthole" cx="330" cy="112" r="3" />
	</g>
</svg>

<style>
	.ship {
		overflow: visible;
	}
	.bob {
		animation: bob 7s ease-in-out infinite;
		transform-origin: 200px 140px;
	}
	@keyframes bob {
		0%,
		100% {
			transform: translateY(0) rotate(0deg);
		}
		50% {
			transform: translateY(3px) rotate(-0.6deg);
		}
	}
	.box,
	.cabin,
	.funnel,
	.hull {
		stroke: var(--line);
		stroke-width: 2;
		stroke-linejoin: round;
	}
	.rib,
	.window,
	.mast,
	.funnel-band {
		fill: none;
		stroke: var(--line);
		stroke-width: 1.5;
		stroke-linecap: round;
	}
	.window {
		stroke-width: 4;
		stroke-dasharray: 5 3;
		stroke: var(--illustration-fill-muted);
	}
	.tone-primary {
		fill: var(--primary);
	}
	.tone-accent {
		fill: var(--accent);
	}
	.tone-muted {
		fill: var(--illustration-fill-muted);
	}
	.tone-raised {
		fill: var(--surface-raised);
	}
	.cabin {
		fill: var(--surface-raised);
	}
	.funnel {
		fill: var(--accent);
	}
	.funnel-band {
		stroke: var(--primary);
		stroke-width: 3;
	}
	.hull {
		fill: var(--illustration-fill-strong);
	}
	.boot {
		fill: var(--accent);
	}
	.porthole {
		fill: var(--primary);
	}
</style>
