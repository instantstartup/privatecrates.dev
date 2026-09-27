/**
 * A deliberately small Markdown renderer for incident write-ups: paragraphs, `-` and `1.` lists, `###` headings,
 * `code`, **strong**, *emphasis* and [links](https://…). Everything else is escaped text. Runs at build time.
 */

export function escapeHtml(text: string): string {
	return text
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

function safeHref(href: string): string | undefined {
	if (href.startsWith('#') || href.startsWith('/')) return href;
	try {
		const url = new URL(href);
		return url.protocol === 'https:' || url.protocol === 'mailto:' ? url.toString() : undefined;
	} catch {
		return undefined;
	}
}

/** Inline formatting, on already-split text. Code spans are taken first so their contents stay literal. */
export function renderInline(text: string): string {
	const parts = text.split(/(`[^`]+`)/);
	return parts
		.map((part) => {
			if (part.length > 2 && part.startsWith('`') && part.endsWith('`')) {
				return `<code>${escapeHtml(part.slice(1, -1))}</code>`;
			}
			let html = escapeHtml(part);
			html = html.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (whole, label: string, href: string) => {
				const safe = safeHref(href.replace(/&amp;/g, '&'));
				return safe ? `<a href="${escapeHtml(safe)}">${label}</a>` : whole;
			});
			html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
			html = html.replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\w)/g, '$1<em>$2</em>');
			return html;
		})
		.join('');
}

export function renderMarkdown(source: string): string {
	const lines = source.replace(/\r\n?/g, '\n').split('\n');
	const out: string[] = [];
	let paragraph: string[] = [];
	let list: { tag: 'ul' | 'ol'; items: string[] } | undefined;

	const flushParagraph = () => {
		if (paragraph.length) out.push(`<p>${renderInline(paragraph.join(' '))}</p>`);
		paragraph = [];
	};
	const flushList = () => {
		if (list) out.push(`<${list.tag}>${list.items.map((i) => `<li>${renderInline(i)}</li>`).join('')}</${list.tag}>`);
		list = undefined;
	};

	for (const raw of lines) {
		const line = raw.trimEnd();
		const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
		const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
		const heading = /^(#{1,6})\s+(.*)$/.exec(line);
		if (line.trim() === '') {
			flushParagraph();
			flushList();
		} else if (heading) {
			flushParagraph();
			flushList();
			// Incident write-ups sit under the page's own h3, so their headings start at h4.
			const level = Math.min(6, Math.max(4, heading[1]!.length + 1));
			out.push(`<h${level}>${renderInline(heading[2]!)}</h${level}>`);
		} else if (bullet || numbered) {
			flushParagraph();
			const tag = bullet ? 'ul' : 'ol';
			if (list && list.tag !== tag) flushList();
			list ??= { tag, items: [] };
			list.items.push((bullet ?? numbered)![1]!);
		} else if (list && /^\s{2,}\S/.test(raw)) {
			// A continuation line of the last list item.
			list.items[list.items.length - 1] += ` ${line.trim()}`;
		} else {
			flushList();
			paragraph.push(line.trim());
		}
	}
	flushParagraph();
	flushList();
	return out.join('\n');
}
