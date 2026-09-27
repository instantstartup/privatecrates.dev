/**
 * The two channels: ntfy (the phone app) for pages, and email for everything. A failure in either is logged and
 * otherwise ignored: alerting must never fail the cron run or the probes.
 *
 * ntfy: `POST {NTFY_URL}/{NTFY_TOPIC}` with the message as a plain-text body and `Title`, `Priority` (1 to 5),
 * `Tags` (comma-separated, emoji short codes) and `Click` headers; `Authorization: Bearer <token>` for a protected
 * topic; RFC 2047 encoded words for non-ASCII header values (https://docs.ntfy.sh/publish/).
 *
 * Email: a `send_email` binding and an `EmailMessage` (from `cloudflare:email`) holding a raw RFC 5322 message, to a
 * verified Email Routing destination
 * (https://developers.cloudflare.com/email-routing/email-workers/send-email-workers/).
 */
import type { AlertMessage } from './alerting.ts';
import type { Fetch } from './probes.ts';

export interface NtfyConfig {
	/** The server, `https://ntfy.sh` by default. */
	url: string;
	/** The topic: on ntfy.sh, a long random name is the credential. */
	topic: string;
	/** An access token (`tk_…`), for a reserved or self-hosted topic. */
	token?: string;
}

/** `new EmailMessage(from, to, raw)`, from `cloudflare:email`; injected so this module runs outside Workers. */
export type MakeEmail = (from: string, to: string, raw: string) => EmailMessage;

export interface EmailConfig {
	binding: SendEmail;
	/** Envelope and header sender, on a domain with Email Routing: `alerts@privatecrates.dev`. */
	from: string;
	/** A verified Email Routing destination address. */
	to: string;
	make: MakeEmail;
}

export interface Channels {
	ntfy?: NtfyConfig;
	email?: EmailConfig;
	fetch: Fetch;
	/** Where failures go: `console.error` in the Worker. */
	log: (message: string) => void;
	/** For the Date header. */
	now: () => number;
	/** For the Message-ID header. */
	id: () => string;
}

export type Outcome = 'sent' | 'failed' | 'off';

export interface Delivery {
	ntfy: Outcome;
	email: Outcome;
}

const NTFY_TIMEOUT_MS = 10_000;
/** ntfy turns a larger body into an attachment (https://docs.ntfy.sh/publish/#attachments). */
const NTFY_MAX_BODY_BYTES = 4096;

const encoder = new TextEncoder();

function isPrintableAscii(s: string): boolean {
	return /^[\x20-\x7e]*$/.test(s);
}

function base64(bytes: Uint8Array): string {
	let binary = '';
	for (const b of bytes) binary += String.fromCharCode(b);
	return btoa(binary);
}

/** No header injection: one line, no control characters. */
function oneLine(s: string): string {
	return s.replace(/[\x00-\x1f\x7f]+/g, ' ').trim();
}

/**
 * RFC 2047 `B` encoded words for a header value with non-ASCII in it, each at most 75 characters and never splitting a
 * character (RFC 2047 §2, §5). ASCII values are returned as they are.
 */
export function encodeWords(value: string): string[] {
	const text = oneLine(value);
	if (isPrintableAscii(text)) return [text];
	const words: string[] = [];
	let chunk: number[] = [];
	const flush = () => {
		if (chunk.length) words.push(`=?UTF-8?B?${base64(Uint8Array.from(chunk))}?=`);
		chunk = [];
	};
	for (const ch of text) {
		const bytes = encoder.encode(ch);
		// 45 bytes of UTF-8 is 60 of base64; with `=?UTF-8?B?` and `?=` that is 72.
		if (chunk.length + bytes.length > 45) flush();
		chunk.push(...bytes);
	}
	flush();
	return words;
}

function truncateUtf8(s: string, maxBytes: number): string {
	if (encoder.encode(s).length <= maxBytes) return s;
	const suffix = '\n[…]';
	let out = '';
	let used = encoder.encode(suffix).length;
	for (const ch of s) {
		const n = encoder.encode(ch).length;
		if (used + n > maxBytes) break;
		out += ch;
		used += n;
	}
	return out + suffix;
}

/** The ntfy publish request for a message. */
export function ntfyRequest(cfg: NtfyConfig, msg: AlertMessage): { url: string; init: RequestInit } {
	const headers: Record<string, string> = {
		'content-type': 'text/plain; charset=utf-8',
		Title: encodeWords(msg.title).join(' '),
		Priority: String(msg.priority),
		Tags: msg.tags.map(oneLine).join(','),
		Click: oneLine(msg.click),
	};
	if (cfg.token) headers.Authorization = `Bearer ${cfg.token}`;
	return {
		url: `${cfg.url.replace(/\/+$/, '')}/${encodeURIComponent(cfg.topic)}`,
		init: { method: 'POST', headers, body: truncateUtf8(msg.body, NTFY_MAX_BODY_BYTES) },
	};
}

export async function sendNtfy(fetcher: Fetch, cfg: NtfyConfig, msg: AlertMessage): Promise<void> {
	const { url, init } = ntfyRequest(cfg, msg);
	const res = await fetcher(url, { ...init, signal: AbortSignal.timeout(NTFY_TIMEOUT_MS) });
	await res.body?.cancel();
	if (!res.ok) throw new Error(`ntfy answered HTTP ${res.status}`);
}

/** RFC 5322 §3.3 date-time, e.g. `Sun, 27 Sep 2026 14:31:00 +0000`. */
export function rfc5322Date(ms: number): string {
	return new Date(ms).toUTCString().replace(/GMT$/, '+0000');
}

function domainOf(address: string): string {
	return address.slice(address.lastIndexOf('@') + 1);
}

export interface MimeInput {
	from: string;
	/** Shown with the sender address: `PrivateCrates status`. */
	fromName: string;
	to: string;
	subject: string;
	body: string;
	dateMs: number;
	/** Unique; becomes `<id@sender-domain>`. */
	id: string;
}

/**
 * A single-part plain-text RFC 5322 message with MIME headers (RFC 2045), CRLF line endings throughout. The body is
 * 7bit when it is ASCII with short lines, and base64 otherwise, so it never needs 8BITMIME.
 */
export function buildMime(m: MimeInput): string {
	const lines = m.body.replace(/\r\n?/g, '\n').split('\n');
	const sevenBit = lines.every((l) => isPrintableAscii(l.replace(/\t/g, ' ')) && l.length <= 998);
	let encoded: string;
	if (sevenBit) {
		encoded = lines.join('\r\n');
	} else {
		const b64 = base64(encoder.encode(lines.join('\r\n')));
		encoded = (b64.match(/.{1,76}/g) ?? []).join('\r\n');
	}
	const name = isPrintableAscii(m.fromName) && !/["\\]/.test(m.fromName) ? `"${m.fromName}"` : encodeWords(m.fromName).join(' ');
	const headers = [
		`From: ${name} <${oneLine(m.from)}>`,
		`To: <${oneLine(m.to)}>`,
		`Subject: ${encodeWords(m.subject).join('\r\n ')}`,
		`Date: ${rfc5322Date(m.dateMs)}`,
		`Message-ID: <${oneLine(m.id)}@${domainOf(m.from)}>`,
		'MIME-Version: 1.0',
		'Content-Type: text/plain; charset=utf-8',
		`Content-Transfer-Encoding: ${sevenBit ? '7bit' : 'base64'}`,
		// RFC 3834: no auto-replies to alerts.
		'Auto-Submitted: auto-generated',
	];
	return `${headers.join('\r\n')}\r\n\r\n${encoded}\r\n`;
}

export async function sendEmail(cfg: EmailConfig, msg: AlertMessage, dateMs: number, id: string): Promise<void> {
	const raw = buildMime({
		from: cfg.from,
		fromName: 'PrivateCrates status',
		to: cfg.to,
		subject: msg.subject,
		body: msg.body,
		dateMs,
		id,
	});
	await cfg.binding.send(cfg.make(cfg.from, cfg.to, raw));
}

/** Sends a message on every configured channel at once. Never throws. */
export async function deliver(channels: Channels, msg: AlertMessage): Promise<Delivery> {
	const run = async (name: string, cfg: unknown, send: () => Promise<void>): Promise<Outcome> => {
		if (!cfg) return 'off';
		try {
			await send();
			return 'sent';
		} catch (e) {
			channels.log(`alert: ${name} failed for "${msg.title}": ${(e as Error)?.message ?? String(e)}`);
			return 'failed';
		}
	};
	const [ntfy, email] = await Promise.all([
		run('ntfy', channels.ntfy, () => sendNtfy(channels.fetch, channels.ntfy!, msg)),
		run('email', channels.email, () => sendEmail(channels.email!, msg, channels.now(), channels.id())),
	]);
	return { ntfy, email };
}
