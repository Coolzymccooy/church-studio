import { changeBody, CONTROL_RETRY_WINDOW_MS, drainControlQueue, pairCodeFromHash } from './protocol.js';
import { createBoard } from './board.js';

const el = (id) => document.getElementById(id);
const names = { desk: 'Desk', monitor1: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3' };
const queue = new Map();
const active = new Set();
const board = createBoard({
  onLevel: (key, target, db) => write(key, target, 'level', db),
  onMute: (key, target, muted) => write(key, target, 'mute', muted),
  busy: (id) => active.has(id) || queue.has(id),
});
let view = null;
let sending = false;
let applyingPair = false;
let events = null;
let connectGeneration = 0;
let renewedAt = 0;
let connectionLost = false;

function selectFinish(finish) {
  document.documentElement.dataset.finish = finish;
  const label = finish === 'bone' ? 'Bone' : 'Graphite';
  el('finish').setAttribute('aria-label', `Finish: ${label}`);
  const chassis = getComputedStyle(document.documentElement).getPropertyValue('--fp-chassis').trim();
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', chassis);
  board.refresh();
}

try {
  selectFinish(localStorage.getItem('tiwaton_remote_finish') === 'bone' ? 'bone' : 'graphite');
} catch {
  selectFinish('graphite');
}
el('finish').onclick = () => {
  const finish = document.documentElement.dataset.finish === 'bone' ? 'graphite' : 'bone';
  selectFinish(finish);
  try { localStorage.setItem('tiwaton_remote_finish', finish); } catch { /* optional storage */ }
};

function pinTip(open) {
  el('helptip').hidden = !open;
  el('help').setAttribute('aria-expanded', String(open));
}
let tipPinned = false;
el('help').onclick = () => { tipPinned = !tipPinned; pinTip(tipPinned); };
el('help').onpointerenter = () => pinTip(true);
el('help').onpointerleave = () => pinTip(tipPinned);
el('help').onfocus = () => pinTip(true);
el('help').onblur = () => { tipPinned = false; pinTip(false); };
document.addEventListener('keydown', (event) => { if (event.key === 'Escape') { tipPinned = false; pinTip(false); } });

function setLink(state) {
  const words = { live: 'Live', off: 'Engine off', lost: 'Offline' };
  const lamps = { live: 'fp-led-g', off: 'fp-led-a', lost: 'fp-led-r' };
  el('message').textContent = words[state];
  el('led').className = `fp-led ${lamps[state]}`;
}

function showError(text) {
  el('error').textContent = text;
  el('error').hidden = !text;
}

function render(next) {
  if (view?.layout !== next.layout || view?.role !== next.role) board.reset();
  view = next;
  el('pair').hidden = true;
  el('mixer').hidden = false;
  el('role').textContent = names[view.role];
  setLink(view.running ? 'live' : 'off');
  board.render(view);
}

function unpair() {
  events?.close();
  events = null;
  view = null;
  queue.clear();
  board.reset();
  document.body.classList.remove('mixing', 'desk');
  el('pair').hidden = false;
  el('mixer').hidden = true;
  el('role').textContent = 'Remote mixer';
  setLink('lost');
}

async function connect() {
  const generation = ++connectGeneration;
  events?.close();
  events = null;
  try {
    const response = await fetch('/api/state');
    if (generation !== connectGeneration || applyingPair) return;
    if (response.status === 401) {
      if (view) showError('Pair again');
      unpair();
      return;
    }
    if (response.ok) receive(await response.json());
    else disconnect();
    startEvents();
  } catch {
    if (generation === connectGeneration) {
      disconnect();
      reconnect(generation);
    }
  }
}

function reconnect(generation) {
  window.setTimeout(() => { if (generation === connectGeneration) void connect(); }, 1500);
}

function receive(next) {
  if (connectionLost) showError('');
  connectionLost = false;
  render(next);
}

function renew() {
  if (!view || document.hidden || Date.now() - renewedAt < 60 * 60 * 1000) return;
  renewedAt = Date.now();
  void fetch('/api/renew', { method: 'POST' }).catch(() => {});
}

function startEvents() {
  events = new EventSource('/api/events');
  events.addEventListener('state', ({ data }) => {
    receive(JSON.parse(data));
    renew();
  });
  events.addEventListener('meters', ({ data }) => {
    if (view) board.meters(JSON.parse(data));
  });
  events.addEventListener('revoked', () => {
    unpair();
    showError('Pair again');
  });
  events.onerror = () => {
    disconnect();
    if (events?.readyState === EventSource.CLOSED) {
      events = null;
      reconnect(connectGeneration);
    }
  };
}

function disconnect() {
  el('pair').hidden = Boolean(view);
  if (view) render({ ...view, running: false });
  setLink('lost');
  connectionLost = true;
  showError('Connection lost');
}

function write(key, target, kind, value) {
  if (!view?.running) return;
  const id = `${key}:${kind}`;
  queue.set(id, {
    change: changeBody(view.layout, target, kind, value),
    generation: connectGeneration,
    retryUntil: performance.now() + CONTROL_RETRY_WINDOW_MS,
  });
  if (!sending) void flush();
}

async function postControl(change) {
  const response = await fetch('/api/control', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(change),
  });
  return response.status;
}

function controlResult(status) {
  if (status === 401) {
    unpair();
    showError('Pair again');
  } else if (status === 409) {
    queue.clear();
    showError('Mixer layout changed');
    void connect();
  } else if (status >= 200 && status < 300) {
    showError('');
  } else {
    showError(status === 423 ? 'Fader in use' : 'Control unavailable');
    if (view) render(view);
  }
}

async function flush() {
  sending = true;
  try {
    await drainControlQueue({
      queue, active, post: postControl, respond: controlResult,
      wait: (ms) => new Promise((resolve) => window.setTimeout(resolve, ms)),
      now: () => performance.now(),
      isCurrent: ({ change, generation }) => generation === connectGeneration && change.layout === view?.layout,
    });
  } catch {
    queue.clear();
    disconnect();
  } finally {
    sending = false;
    if (queue.size) void flush();
  }
}

el('pair').onsubmit = async (event) => {
  event.preventDefault();
  if (applyingPair) return;
  applyingPair = true;
  renewedAt = 0;
  showError('');
  const button = event.currentTarget.querySelector('button');
  button.disabled = true;
  const code = el('code').value;
  el('code').value = '';
  try {
    const response = await fetch('/api/pair', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, name: el('name').value.trim(), renew: el('renew').checked }),
    });
    if (!response.ok) {
      const errors = { 401: 'Code invalid or expired', 429: 'Wait and retry' };
      showError(errors[response.status] ?? 'Pairing failed');
    }
  } catch {
    showError('Connection lost');
  } finally {
    applyingPair = false;
    button.disabled = false;
  }
  void connect();
};

const codeFromQr = pairCodeFromHash(window.location.hash);
if (window.location.hash) window.history.replaceState(null, '', '/');
el('code').value = codeFromQr;
void fetch('/api/identity')
  .then((response) => response.ok ? response.json() : null)
  .then((identity) => { if (identity?.fingerprint) el('fingerprint').textContent = identity.fingerprint; })
  .catch(() => {});
document.addEventListener('visibilitychange', () => {
  if (!document.hidden && view && !events) void connect();
  renew();
});
window.setInterval(renew, 60 * 60 * 1000);
window.setInterval(() => { if (view) board.tick(); }, 100);
void connect();
