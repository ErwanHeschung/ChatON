// Official Twemoji SVGs (jdecked/twemoji, the maintained fork) into public/twemoji, served by the
// app itself: no CDN (CSP, privacy, offline). Pinned tag and checksum; runs before start and build,
// and does nothing once this version is there. To update: bump TAG and SHA256, and @twemoji/parser.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const TAG = '17.0.3';
const SHA256 = 'a0855654b633045ae2337537e77f1bb4361162f7fcd910e613eaab1d6d9c5fca';

const out = fileURLToPath(new URL('../public/twemoji/', import.meta.url));
const stamp = join(out, '.version');
if (existsSync(stamp) && readFileSync(stamp, 'utf8') === TAG) {
  process.exit(0);
}

const url = `https://codeload.github.com/jdecked/twemoji/tar.gz/refs/tags/v${TAG}`;
const res = await fetch(url);
if (!res.ok) {
  throw new Error(`Twemoji ${TAG}: ${url} answered ${res.status}`);
}
const tgz = Buffer.from(await res.arrayBuffer());
const sum = createHash('sha256').update(tgz).digest('hex');
if (sum !== SHA256) {
  throw new Error(`Twemoji ${TAG}: checksum ${sum}, expected ${SHA256}`);
}

const archive = join(tmpdir(), `twemoji-${TAG}.tar.gz`);
writeFileSync(archive, tgz);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
// tar ships with Linux, macOS and Windows 10+.
execFileSync('tar', [
  '-xzf',
  archive,
  '-C',
  out,
  '--strip-components=3',
  `twemoji-${TAG}/assets/svg`,
]);
rmSync(archive);
writeFileSync(stamp, TAG);
console.log(`Twemoji ${TAG}: SVGs in public/twemoji`);
