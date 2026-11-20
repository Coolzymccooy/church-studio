#!/usr/bin/env node
// Fetch the official NDI® runtime library for this platform from NDI's SDK
// download and place it in `src-tauri/ndi-runtime/<platform>/`, where the Tauri
// bundle config picks it up (macOS: Contents/Frameworks; Windows: next to the exe).
//
//   node scripts/fetch-ndi-runtime.mjs            # no-op when already present
//   node scripts/fetch-ndi-runtime.mjs --refresh  # download again if NDI updated it
//   node scripts/fetch-ndi-runtime.mjs --print-url  # the SDK download for this platform (CI cache key)
//
// Runs from `beforeDevCommand` / `beforeBuildCommand`, so `tauri dev` and
// `tauri build` always have it. Downloads are cached (see `defaultCacheDir`) so
// worktrees share one copy. Needs: macOS `xar`, `tar`, `lipo`, `codesign`
// (all built in); Windows `innoextract` and PowerShell.
//
// NDI® is a registered trademark of Vizrt NDI AB (https://ndi.video).
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import {
  PLATFORMS,
  defaultCacheDir,
  isUniversal,
  pickFile,
  platformFor,
  signedByNdiOnWindows,
  signedByNdiTeam,
} from './ndi-runtime-plan.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const runtimeRoot = path.join(repoRoot, 'src-tauri', 'ndi-runtime');

const log = (msg) => console.log(`[ndi-runtime] ${msg}`);

function parseArgs(argv) {
  const args = { refresh: false, printUrl: false, platform: platformFor(process.platform, process.arch) };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--refresh') args.refresh = true;
    else if (argv[i] === '--print-url') args.printUrl = true;
    else if (argv[i] === '--platform') args.platform = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  if (args.platform && !PLATFORMS[args.platform]) throw new Error(`unknown platform: ${args.platform}`);
  return args;
}

function run(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...opts });
}

function isPresent(plan, outDir) {
  return plan.files.every((f) => f.optional || fs.existsSync(path.join(outDir, f.out)));
}

async function headEtag(url) {
  const res = await fetch(url, { method: 'HEAD', redirect: 'follow' });
  if (!res.ok) throw new Error(`HEAD ${url}: HTTP ${res.status}`);
  return { etag: res.headers.get('etag') || '', lastModified: res.headers.get('last-modified') || '' };
}

/** Download `plan.url` into the cache unless the cached copy has the same ETag. */
async function download(plan, cacheDir) {
  fs.mkdirSync(cacheDir, { recursive: true });
  const file = path.join(cacheDir, plan.archive);
  const metaFile = `${file}.json`;
  const cachedMeta = fs.existsSync(metaFile) ? JSON.parse(fs.readFileSync(metaFile, 'utf8')) : null;
  let remote;
  try {
    remote = await headEtag(plan.url);
  } catch (err) {
    if (cachedMeta && fs.existsSync(file)) {
      log(`cannot reach NDI (${err.message}); using the cached download`);
      return { file, ...cachedMeta };
    }
    throw err;
  }
  if (cachedMeta && fs.existsSync(file) && remote.etag && cachedMeta.etag === remote.etag) {
    log(`using cached ${plan.archive}`);
    return { file, ...cachedMeta };
  }
  log(`downloading ${plan.url}`);
  const res = await fetch(plan.url, { redirect: 'follow' });
  if (!res.ok || !res.body) throw new Error(`GET ${plan.url}: HTTP ${res.status}`);
  const part = `${file}.part`;
  await pipeline(Readable.fromWeb(res.body), fs.createWriteStream(part));
  fs.renameSync(part, file);
  const meta = { url: plan.url, etag: remote.etag, lastModified: remote.lastModified };
  fs.writeFileSync(metaFile, `${JSON.stringify(meta, null, 2)}\n`);
  return { file, ...meta };
}

function extractMac(plan, archive, workDir) {
  const payloads = run('xar', ['-tf', archive]).split('\n').filter((e) => e.endsWith('/Payload'));
  if (payloads.length === 0) throw new Error('no Payload in the NDI SDK package');
  const root = path.join(workDir, 'root');
  fs.mkdirSync(root, { recursive: true });
  const patterns = plan.files.map((f) => `*/${f.find}`);
  for (const payload of payloads) {
    run('xar', ['-xf', archive, '-C', workDir, payload]);
    // bsdtar: extract only the files we need from the (~2 GB) payload.
    try {
      run('tar', ['-xf', path.join(workDir, payload), '-C', root, ...patterns], { stdio: 'pipe' });
    } catch (err) {
      // tar exits non-zero when an optional pattern matches nothing; the
      // required files are checked afterwards.
      log(`tar: ${String(err.stderr || err.message).trim().split('\n').pop()}`);
    }
  }
  return root;
}

function extractWindows(archive, workDir) {
  try {
    run('innoextract', ['--version'], { stdio: 'pipe' });
  } catch {
    throw new Error('innoextract is needed to unpack the NDI SDK installer: choco install innoextract (or scoop install innoextract)');
  }
  const root = path.join(workDir, 'root');
  run('innoextract', ['--silent', '--output-dir', root, archive], { stdio: 'pipe' });
  return root;
}

function listFiles(root) {
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile()) out.push(path.relative(root, full).split(path.sep).join('/'));
    }
  };
  walk(root);
  return out;
}

function verifyMacLibrary(file) {
  const archs = run('lipo', ['-archs', file]).trim();
  if (!isUniversal(archs)) throw new Error(`libndi.dylib is not universal (has: ${archs})`);
  run('codesign', ['--verify', '--strict', file], { stdio: 'pipe' });
  // codesign -dv reports on stderr.
  const details = run('sh', ['-c', 'codesign -dv "$1" 2>&1', 'sh', file]);
  if (!signedByNdiTeam(details)) throw new Error('libndi.dylib is not signed by NDI');
  return { archs };
}

function verifyWindowsLibrary(file) {
  const script = '$s = Get-AuthenticodeSignature -LiteralPath $env:NDI_DLL; '
    + '"$($s.Status)|$($s.SignerCertificate.Subject)|$((Get-Item -LiteralPath $env:NDI_DLL).VersionInfo.FileVersion)"';
  const [status, subject, fileVersion] = run(
    'powershell', ['-NoProfile', '-NonInteractive', '-Command', script],
    { env: { ...process.env, NDI_DLL: file } },
  ).trim().split('|');
  if (status !== 'Valid') throw new Error(`NDI DLL signature is ${status}`);
  if (!signedByNdiOnWindows(subject)) throw new Error(`NDI DLL is signed by someone else: ${subject}`);
  return { signer: subject, fileVersion };
}

function sha256(file) {
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function install(plan, root, outDir, source) {
  const available = listFiles(root);
  const tmpOut = `${outDir}.tmp`;
  fs.rmSync(tmpOut, { recursive: true, force: true });
  fs.mkdirSync(tmpOut, { recursive: true });
  const manifest = { source: source.url, etag: source.etag, lastModified: source.lastModified, files: {} };
  for (const spec of plan.files) {
    const picked = pickFile(spec, available);
    if (!picked) {
      if (spec.optional) continue;
      throw new Error(`${spec.find} not found in ${plan.archive}`);
    }
    const dest = path.join(tmpOut, spec.out);
    fs.copyFileSync(path.join(root, picked), dest);
    manifest.files[spec.out] = { from: picked, sha256: sha256(dest) };
    if (spec.library) {
      manifest.check = plan.dir === 'macos' ? verifyMacLibrary(dest) : verifyWindowsLibrary(dest);
    }
  }
  const versionFile = path.join(tmpOut, 'Version.txt');
  manifest.version = fs.existsSync(versionFile)
    ? fs.readFileSync(versionFile, 'utf8').trim()
    : manifest.check?.fileVersion || null;
  fs.writeFileSync(path.join(tmpOut, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.renameSync(tmpOut, outDir);
  return manifest;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.platform) {
    log(`nothing is bundled on ${process.platform}/${process.arch}; skipping`);
    return;
  }
  const plan = PLATFORMS[args.platform];
  if (args.printUrl) {
    console.log(plan.url);
    return;
  }
  const outDir = path.join(runtimeRoot, plan.dir);
  if (!args.refresh && isPresent(plan, outDir)) {
    log(`${plan.dir} runtime present (${path.relative(repoRoot, outDir)})`);
    return;
  }
  const cacheDir = defaultCacheDir(process.env, process.platform, os.homedir());
  const source = await download(plan, cacheDir);
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ndi-runtime-'));
  try {
    const root = plan.dir === 'macos'
      ? extractMac(plan, source.file, workDir)
      : extractWindows(source.file, workDir);
    const manifest = install(plan, root, outDir, source);
    log(`installed NDI runtime ${manifest.version || '(unknown version)'} into ${path.relative(repoRoot, outDir)}`);
  } finally {
    fs.rmSync(workDir, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error(`[ndi-runtime] ${err.message}`);
  process.exit(1);
});
