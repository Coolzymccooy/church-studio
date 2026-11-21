#!/usr/bin/env node
// Preview server for the remote mixer page, with a fake board and synthetic meters, so the
// phone and tablet layouts can be checked in any browser without the desktop app.
//   node scripts/remote-mixer-preview.mjs [port]
// Open /__as/monitor2 (or monitor1, monitor3, desk) to pair as that role, /__as/none to unpair.
// Plain HTTP on localhost only: it has none of the real listener's TLS, pairing or auth.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const web = join(root, 'src-tauri/src/remote_mixer/web');
const port = Number(process.argv[2] ?? 4476);
const ROLES = ['monitor1', 'monitor2', 'monitor3', 'desk'];
const SEND_KEY = { monitor1: 'mon1', monitor2: 'mon2', monitor3: 'mon3' };
const BUSES = ['main', 'stream', 'monitor', 'monitor2', 'monitor3'];
const BUS_NAMES = { main: 'Main', stream: 'Stream', monitor: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3' };

const channels = [
  ['Pastor', -14, 0.0], ['Worship lead', -11, -2.5], ['Acoustic gtr', -16, -6], ['Keys', -18, -4],
  ['Bass', -12, -9], ['Drums', -7, -12], ['Choir', -20, -15.5], ['Playback', -15, -90],
].map(([name, level, send], index) => ({
  index, name, level, fader: -index * 2, mute: index === 6,
  mon1: send, mon2: send, mon3: send - 3,
}));
const buses = Object.fromEntries(BUSES.map((id) => [id, { fader: 0, mute: false }]));
const clients = new Set();

const files = {
  '/': ['index.html', 'text/html; charset=utf-8'],
  '/desk.js': ['desk.js', 'application/javascript'],
  '/protocol.js': ['protocol.js', 'application/javascript'],
  '/board.js': ['board.js', 'application/javascript'],
  '/fader.js': ['fader.js', 'application/javascript'],
  '/meters.js': ['meters.js', 'application/javascript'],
  '/mixmath.js': ['mixmath.js', 'application/javascript'],
  '/font.woff2': ['legend.woff2', 'font/woff2'],
  '/legend-600.woff2': ['legend-600.woff2', 'font/woff2'],
  '/mono.woff2': ['mono.woff2', 'font/woff2'],
  '/mono-500.woff2': ['mono-500.woff2', 'font/woff2'],
};

function css() {
  return ['src/styles/faceplate-tokens.css', 'src/styles/faceplate-parts.css']
    .map((file) => readFileSync(join(root, file), 'utf8')).join('') + readFileSync(join(web, 'desk.css'), 'utf8');
}

function view(role) {
  const desk = role === 'desk';
  const controls = channels.map((ch) => ({
    target: { type: 'strip', index: ch.index }, name: ch.name,
    db: desk ? ch.fader : ch[SEND_KEY[role]], ...(desk ? { muted: ch.mute } : {}),
  }));
  if (desk) {
    for (const id of BUSES) controls.push({ target: { type: 'bus', id }, name: BUS_NAMES[id], db: buses[id].fader, muted: buses[id].mute });
    controls.push({ target: { type: 'return', id: 'fx_a' }, name: 'FX A', db: -6, muted: false });
  } else {
    const bus = { monitor1: 'monitor', monitor2: 'monitor2', monitor3: 'monitor3' }[role];
    controls.push({ target: { type: 'personal' }, name: BUS_NAMES[bus], db: 0, muted: buses[bus].mute });
  }
  return { role, running: true, layout: 'preview', controls };
}

const sum = (dbs) => 10 * Math.log10(dbs.reduce((acc, db) => acc + (Number.isFinite(db) ? 10 ** (db / 10) : 0), 1e-12));
const round = (db) => Math.round(Math.max(-96, db) * 10) / 10;

function meters(role, t) {
  const taps = channels.map((ch, i) => {
    const swell = 5 * Math.sin(t * (0.6 + i * 0.13) + i) + 3 * Math.sin(t * (3.1 + i * 0.7));
    const hit = ch.name === 'Drums' && (t % 2.4) < 0.12 ? 7.5 : 0;
    return round(Math.min(0, ch.level + swell + hit));
  });
  if (role === 'desk') {
    const post = taps.map((tap, i) => (channels[i].mute ? -96 : round(tap + channels[i].fader)));
    const main = round(sum(post) + 3);
    return { strips: post, buses: Object.fromEntries(BUSES.map((id, i) => [id, [main - i, main - i - 0.8]])) };
  }
  const key = SEND_KEY[role];
  const inMix = taps.map((tap, i) => (channels[i].mute || channels[i][key] <= -90 ? -Infinity : tap + channels[i][key]));
  const rms = sum(inMix) - 2;
  const peak = Math.min(1.2, rms + 7 + 2 * Math.sin(t * 7));
  return {
    strips: taps.map((tap, i) => (channels[i].mute ? -96 : tap)),
    mix: [round(peak), round(peak - 1.1), round(rms), round(rms - 0.9)],
  };
}

function send(res, status, body = '', type = 'application/json') {
  res.writeHead(status, { 'content-type': type, 'cache-control': 'no-store' });
  res.end(body);
}

function roleOf(req) {
  const match = /(?:^|; )mock_role=(\w+)/.exec(req.headers.cookie ?? '');
  return ROLES.includes(match?.[1]) ? match[1] : null;
}

function publish() {
  for (const client of clients) client.res.write(`event: state\ndata: ${JSON.stringify(view(client.role))}\n\n`);
}

function control(role, change) {
  const { target, kind } = change;
  if (target.type === 'personal' && kind === 'mute') {
    buses[{ monitor1: 'monitor', monitor2: 'monitor2', monitor3: 'monitor3' }[role]].mute = change.muted;
  } else if (target.type === 'strip') {
    const ch = channels[target.index];
    if (kind === 'level') ch[role === 'desk' ? 'fader' : SEND_KEY[role]] = change.db;
    else if (role === 'desk') ch.mute = change.muted;
  } else if (target.type === 'bus' && role === 'desk') {
    if (kind === 'level') buses[target.id].fader = change.db; else buses[target.id].mute = change.muted;
  }
  publish();
}

createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const role = roleOf(req);
  if (url.pathname.startsWith('/__as/')) {
    const as = url.pathname.slice(6);
    res.writeHead(302, { location: '/', 'set-cookie': `mock_role=${ROLES.includes(as) ? as : ''}; Path=/` });
    return res.end();
  }
  if (url.pathname === '/desk.css') return send(res, 200, css(), 'text/css; charset=utf-8');
  if (files[url.pathname]) {
    const [file, type] = files[url.pathname];
    return send(res, 200, readFileSync(join(web, file)), type);
  }
  if (url.pathname === '/api/identity') return send(res, 200, JSON.stringify({ fingerprint: 'AB:12:CD:34:EF:56:78:90:AB:12:CD:34:EF:56:78:90' }));
  if (url.pathname === '/api/pair' && req.method === 'POST') {
    res.writeHead(204, { 'set-cookie': 'mock_role=monitor2; Path=/' });
    return res.end();
  }
  if (!role) return send(res, 401);
  if (url.pathname === '/api/state') return send(res, 200, JSON.stringify(view(role)));
  if (url.pathname === '/api/renew') return send(res, 204);
  if (url.pathname === '/api/control' && req.method === 'POST') {
    let body = '';
    req.on('data', (chunk) => { body += chunk; });
    req.on('end', () => { control(role, JSON.parse(body)); send(res, 204); });
    return undefined;
  }
  if (url.pathname === '/api/events') {
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store' });
    const client = { res, role };
    clients.add(client);
    res.write(`event: state\ndata: ${JSON.stringify(view(role))}\n\n`);
    const timer = setInterval(() => {
      res.write(`event: meters\ndata: ${JSON.stringify(meters(role, performance.now() / 1000))}\n\n`);
    }, 60);
    req.on('close', () => { clearInterval(timer); clients.delete(client); });
    return undefined;
  }
  return send(res, 404);
}).listen(port, '127.0.0.1', () => console.log(`remote mixer preview on http://127.0.0.1:${port}/__as/monitor2`));
