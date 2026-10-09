<script lang="ts">
	import { page } from '$app/state';
	import { primaryNav } from '$lib/site';
	import Logo from './Logo.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	function isCurrent(href: string): boolean {
		const path = page.url.pathname;
		return path === href || path.startsWith(href + '/');
	}
</script>

<header class="site-header">
	<div class="page bar">
		<Logo />
		<nav aria-label="Main">
			<ul>
				{#each primaryNav as link (link.href)}
					<li>
						<a href={link.href} aria-current={isCurrent(link.href) ? 'page' : undefined}>{link.label}</a>
					</li>
				{/each}
				<li>
					<a href="/account" class="account" aria-current={isCurrent('/account') ? 'page' : undefined}
						>Account</a
					>
				</li>
			</ul>
		</nav>
		<ThemeToggle />
	</div>
</header>

<style lang="postcss">
	@reference '../../app.css';

	.site-header {
		border-bottom: var(--border-outline) solid var(--line);
		background: var(--surface);
	}
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: var(--space-5);
		padding-block: var(--space-3);
	}
	nav {
		margin-left: auto;
	}
	ul {
		display: flex;
		gap: var(--space-2);
	}
	a:not(:global(.logo)) {
		display: inline-flex;
		align-items: center;
		min-height: var(--size-control);
		padding-inline: var(--space-menu-x);
		border-radius: var(--radius-control);
		color: var(--ink);
		font-weight: 700;
		text-decoration: none;
	}
	a:not(:global(.logo)):hover {
		background: color-mix(in srgb, var(--ink) 7%, transparent);
	}
	a[aria-current='page'] {
		text-decoration: underline;
		text-decoration-thickness: var(--border-strong);
		text-decoration-color: var(--action);
		text-underline-offset: var(--underline-offset-current);
	}
	.account {
		border: var(--border-outline) solid var(--line);
		margin-left: var(--space-2);
	}

	/* Narrow phones: the nav drops to its own row under the logo. */
	@media (width <= theme(--breakpoint-1)) {
		.bar {
			justify-content: space-between;
		}
		nav {
			order: 3;
			width: 100%;
			margin-left: 0;
			border-top: var(--border-hairline) solid var(--divider);
			padding-top: var(--space-2);
		}
		ul {
			justify-content: space-between;
		}
		a:not(:global(.logo)) {
			padding-inline: var(--space-3);
		}
	}
</style>
