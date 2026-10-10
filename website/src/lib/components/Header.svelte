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

<style>
	.site-header {
		border-bottom: 2px solid var(--line);
		background: var(--surface);
	}
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 1rem;
		padding-block: 0.5rem;
	}
	nav {
		margin-left: auto;
	}
	ul {
		display: flex;
		gap: 0.25rem;
	}
	a:not(:global(.logo)) {
		display: inline-flex;
		align-items: center;
		min-height: 2.75rem;
		padding-inline: 0.7rem;
		border-radius: 6px;
		color: var(--ink);
		font-weight: 700;
		text-decoration: none;
	}
	a:not(:global(.logo)):hover {
		background: color-mix(in srgb, var(--ink) 7%, transparent);
	}
	a[aria-current='page'] {
		text-decoration: underline;
		text-decoration-thickness: 3px;
		text-decoration-color: var(--action);
		text-underline-offset: 0.35em;
	}
	.account {
		border: 2px solid var(--line);
		margin-left: 0.25rem;
	}

	/* Narrow phones: the nav drops to its own row under the logo. */
	@media (max-width: 30rem) {
		.bar {
			justify-content: space-between;
		}
		nav {
			order: 3;
			width: 100%;
			margin-left: 0;
			border-top: 1px solid var(--divider);
			padding-top: 0.25rem;
		}
		ul {
			justify-content: space-between;
		}
		a:not(:global(.logo)) {
			padding-inline: 0.5rem;
		}
	}
</style>
