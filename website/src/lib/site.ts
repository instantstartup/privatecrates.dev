/** The production origin, used for canonical links, Open Graph and the sitemap. */
export const SITE_URL = 'https://privatecrates.dev';
export const SITE_NAME = 'PrivateCrates';

/**
 * The preview (docs/preview.md): free, as is, at your own risk, operated by an individual until a company exists.
 * Static pages read this; the account page follows the server's `session.preview`.
 */
export const PREVIEW = true;
/** The current private preview terms: the version an admin accepts, and when it was published. */
export const TERMS_VERSION = 'private-preview-2026-09-28';
export const TERMS_DATE = '28 September 2026';
export const TERMS_PATH = '/legal/terms';
/** The operator during the private preview. */
export const OPERATOR = 'Bryn Dyllan Cooke';
/** When general availability, and so billing, is planned. */
export const GA_TARGET = '2027, subject to demand';

/** Price per GitHub organisation per month, in US dollars. */
export const PRICE_USD = 100;
/** Organisations with this many active members or fewer are free (FREE_MEMBER_LIMIT on the server). */
export const FREE_MEMBER_LIMIT = 5;
/** Length of the no-card free trial for larger organisations (TRIAL_DAYS on the server). */
export const TRIAL_DAYS = 90;
export const TRIAL_MONTHS = 3;
/** The trial banner and publish warnings start this many days before a trial without a card ends. */
export const TRIAL_REMINDER_DAYS = 14;
/** Reads keep working for this long after an inactive subscription's period ends. */
export const READ_GRACE_DAYS = 14;

export interface NavLink {
	href: string;
	label: string;
}

export const primaryNav: NavLink[] = [
	{ href: '/docs', label: 'Docs' },
	{ href: '/pricing', label: 'Pricing' },
	{ href: '/trust', label: 'Trust' }
];

export const docsNav: NavLink[] = [
	{ href: '/docs', label: 'Overview' },
	{ href: '/docs/setup', label: 'Set up a registry' },
	{ href: '/docs/joining', label: 'Joining a team' },
	{ href: '/docs/agents', label: 'Set up with an AI agent' },
	{ href: '/docs/ci', label: 'CI without secrets' },
	{ href: '/docs/publishing', label: 'Publishing' },
	{ href: '/docs/verify', label: 'Verify the registry' },
	{ href: '/docs/security', label: 'Security model' },
	{ href: '/docs/errors', label: 'Error reference' }
];

/** Every prerendered, indexable page, for sitemap.xml. */
export const sitemapPaths = [
	'/',
	'/pricing',
	...docsNav.map((l) => l.href).filter((h) => h !== '/docs'),
	'/docs',
	'/trust',
	'/security',
	'/legal/terms',
	'/legal/privacy',
	'/legal/dpa'
];

/** Registry name used in examples throughout the docs. */
export const EXAMPLE_SLUG = 'acme';
