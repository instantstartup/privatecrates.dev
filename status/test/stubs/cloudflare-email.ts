/** `cloudflare:email` exists only in workerd; under Vitest (plain Node) this stands in for it (vitest.config.ts). */
export class EmailMessage {
	readonly from: string;
	readonly to: string;
	readonly raw: string;
	constructor(from: string, to: string, raw: string) {
		this.from = from;
		this.to = to;
		this.raw = raw;
	}
}
