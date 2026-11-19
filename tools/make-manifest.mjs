#!/usr/bin/env node
// Tạo manifest.json cho bộ dữ liệu game (dùng cho tính năng tải dữ liệu lần đầu của AetherLaunch).
//
// Dùng:
//   node tools/make-manifest.mjs <thư-mục-dữ-liệu> --version 1.0.0 [--name "Tên bộ dữ liệu"] [--base-url https://cdn.example.com/pack/] [--out manifest.json]
//
// - <thư-mục-dữ-liệu>: thư mục sẽ được copy nguyên vào máy người chơi (phải chứa gta_sa.exe nếu muốn launcher tự nhận game).
// - manifest.json được ghi cạnh thư mục dữ liệu (hoặc theo --out). Upload cả hai lên server: manifest.json + các file.
// - Không cần --base-url nếu các file nằm cùng thư mục với manifest.json trên server.
// - Bỏ qua: .git, .DS_Store, Thumbs.db, desktop.ini, *.part và manifest.json.
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readdir, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';

const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const dir = args.find((a, i) => !a.startsWith('--') && !(i > 0 && args[i - 1].startsWith('--')));
if (!dir || !opt('--version')) {
  console.error('Cách dùng: node tools/make-manifest.mjs <thư-mục> --version 1.0.0 [--name "..."] [--base-url https://...] [--out manifest.json]');
  process.exit(1);
}
const root = path.resolve(dir);
const out = path.resolve(opt('--out', path.join(path.dirname(root), 'manifest.json')));
const SKIP = new Set(['.git', '.ds_store', 'thumbs.db', 'desktop.ini', 'manifest.json']);

async function* walk(d) {
  for (const e of (await readdir(d, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
    if (SKIP.has(e.name.toLowerCase()) || e.name.toLowerCase().endsWith('.part')) continue;
    const p = path.join(d, e.name);
    if (e.isDirectory()) yield* walk(p); else if (e.isFile()) yield p;
  }
}
const sha256 = f => new Promise((res, rej) => {
  const h = createHash('sha256'); const s = createReadStream(f);
  s.on('data', c => h.update(c)); s.on('end', () => res(h.digest('hex'))); s.on('error', rej);
});
const bad = /[\\:<>"|?*\x00-\x1f]|\/\/|(^|\/)\.\.?(\/|$)|[. ](\/|$)/;

const files = []; let total = 0;
for await (const f of walk(root)) {
  const rel = path.relative(root, f).split(path.sep).join('/');
  if (bad.test(rel)) { console.error(`Bỏ qua (tên không hợp lệ cho Windows): ${rel}`); continue; }
  const size = (await stat(f)).size;
  process.stdout.write(`\r${files.length + 1} file… ${rel.slice(0, 60).padEnd(60)}`);
  files.push({ path: rel, size, sha256: await sha256(f) });
  total += size;
}
const manifest = { name: opt('--name', 'Dữ liệu game'), version: opt('--version'), ...(opt('--base-url') ? { baseUrl: opt('--base-url') } : {}), files };
await writeFile(out, JSON.stringify(manifest, null, 2) + '\n');
console.log(`\nXong: ${files.length} file, ${(total / 1048576).toFixed(1)} MB -> ${out}`);
if (!files.some(f => f.path.toLowerCase() === 'gta_sa.exe')) console.warn('Lưu ý: bộ dữ liệu không có gta_sa.exe ở thư mục gốc, launcher sẽ không tự nhận game sau khi tải.');
