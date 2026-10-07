// Writes Brotli (.br) and gzip (.gz) copies of the built website's text files, at the highest settings: they are
// made once here, and the server sends them as they are (ServeDir's precompressed files), so compressing the
// website costs no CPU per request. Small files, and those that do not shrink, are left alone.
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const root = new URL('../build/', import.meta.url).pathname;
const compressible = /\.(html|js|css|svg|txt|json|xml|webmanifest)$/;
const minimum = 1024;

/** @param {string} dir */
function walk(dir) {
	for (const name of readdirSync(dir)) {
		const path = join(dir, name);
		if (statSync(path).isDirectory()) walk(path);
		else if (compressible.test(name)) compress(path);
	}
}

/** @param {string} path */
function compress(path) {
	const content = readFileSync(path);
	if (content.length < minimum) return;
	const br = brotliCompressSync(content, {
		params: {
			[constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
			[constants.BROTLI_PARAM_SIZE_HINT]: content.length
		}
	});
	const gz = gzipSync(content, { level: 9 });
	if (br.length < content.length) writeFileSync(`${path}.br`, br);
	if (gz.length < content.length) writeFileSync(`${path}.gz`, gz);
}

walk(root);
