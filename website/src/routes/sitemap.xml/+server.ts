import { SITE_URL, sitemapPaths } from '$lib/site';

export const prerender = true;

export function GET(): Response {
	const urls = sitemapPaths
		.map((path) => `  <url><loc>${SITE_URL}${path === '/' ? '/' : path}</loc></url>`)
		.join('\n');
	const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`;
	return new Response(body, { headers: { 'Content-Type': 'application/xml' } });
}
