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
