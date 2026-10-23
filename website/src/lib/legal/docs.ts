import { PREVIEW, TERMS_PATH } from '$lib/site';

/**
 * The legal documents that apply once billing is on (PREVIEW is false): when each was last changed, and the version
 * shown on its page. The terms' own version, the one an admin accepts, is TERMS_VERSION in $lib/site.
 */
export const PAID_TERMS_DATE = '22 October 2026';
export const REFUNDS_PATH = '/legal/refunds';

export interface LegalDoc {
	version: string;
	date: string;
}

export const REFUNDS_DOC: LegalDoc = { version: '2026-10-22', date: '22 October 2026' };
export const PRIVACY_DOC: LegalDoc = { version: '2026-10-22', date: '22 October 2026' };
export const DPA_DOC: LegalDoc = { version: '2026-10-22', date: '22 October 2026' };

export interface LegalDocLink {
	href: string;
	/** The document's name, as a link on the trust centre. */
	title: string;
	/** Its name in the footer. */
	short: string;
	/** Whether, and how, it is in force: shown after the name on the trust centre. */
	status: string;
}

/**
 * The legal documents and whether each is in force, for the trust centre and the footer. Every statement about a
 * document's status lives here, beside the PREVIEW switch the documents themselves follow, so the pages that list
 * them cannot call a document a draft once it is in force, or the other way round.
 */
export const legalDocuments: LegalDocLink[] = PREVIEW
	? [
			{
				href: TERMS_PATH,
				title: 'Preview terms',
				short: 'Preview terms',
				status: 'in force during the preview'
			},
			{ href: '/legal/privacy', title: 'Privacy notice', short: 'Privacy', status: 'in force' },
			{
				href: '/legal/dpa',
				title: 'Data processing agreement',
				short: 'DPA (draft)',
				status: 'a template, in draft for legal review, not yet in force during the preview'
			}
		]
	: [
			{ href: TERMS_PATH, title: 'Terms of service', short: 'Terms', status: 'in force' },
			{
				href: REFUNDS_PATH,
				title: 'Refunds and cancellation',
				short: 'Refunds and cancellation',
				status: 'part of the terms'
			},
			{ href: '/legal/privacy', title: 'Privacy notice', short: 'Privacy', status: 'in force' },
			{
				href: '/legal/dpa',
				title: 'Data processing agreement',
				short: 'DPA',
				status: 'in force as part of the terms, with the EU Standard Contractual Clauses and the UK Addendum'
			}
		];
