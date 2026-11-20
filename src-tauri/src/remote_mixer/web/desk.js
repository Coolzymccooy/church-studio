import { changeBody, dbLabel, pairCodeFromHash } from './protocol.js';

const el = (id) => document.getElementById(id);
const names = { desk: 'Desk', monitor1: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3' };
const channels = el('channels');
const buses = el('buses');
const map = new Map();
const queue = new Map();
const active = new Set();
const held = new Set();
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
  el('finish').textContent = label;
  el('finish').setAttribute('aria-label', `Finish: ${label}`);
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

function showError(text) {
  el('error').textContent = text;
  el('error').hidden = !text;
}

function targetKey(target) {
  return `${target.type}:${target.index ?? target.id}`;
}

function makeRow(item, key) {
  const row = document.createElement('div');
  row.className = 'channel';
  const readout = document.createElement('div');
  readout.className = 'readout';
  const name = document.createElement('span');
  name.className = 'channel-name';
  const value = document.createElement('span');
  value.className = 'value';
  readout.append(name, value);
  const input = document.createElement('input');
  input.type = 'range';
  input.className = 'level';
  input.min = '-90';
  input.max = '10';
  input.step = '0.1';
  input.onpointerdown = () => {
    const id = `${key}:level`;
    held.add(id);
    const release = () => {
      held.delete(id);
      window.removeEventListener('pointerup', release);
      window.removeEventListener('pointercancel', release);
      window.removeEventListener('blur', release);
    };
    window.addEventListener('pointerup', release);
    window.addEventListener('pointercancel', release);
    window.addEventListener('blur', release);
  };
  input.oninput = () => {
    const db = Number(input.value);
    value.textContent = dbLabel(db);
    input.setAttribute('aria-valuetext', dbLabel(db));
    write(key, item.target, 'level', db);
  };
  const personal = item.target.type === 'personal';
  row.append(readout);
  if (!personal) row.append(input);
  const mute = item.muted === undefined ? null : document.createElement('button');
  if (mute) {
    mute.type = 'button';
    mute.className = 'fp-key';
    mute.textContent = personal ? 'Mute mix' : 'mute';
    mute.onclick = () => {
      const muted = mute.getAttribute('aria-pressed') !== 'true';
      setMute(mute, muted);
      write(key, item.target, 'mute', muted);
    };
    row.append(mute);
  }
  (item.target.type === 'strip' ? channels : buses).append(row);
  return { row, name, value, input: personal ? null : input, mute };
}

function setMute(button, muted) {
  button.setAttribute('aria-pressed', muted);
  button.classList.toggle('fp-on-amber', muted);
}

function render(next) {
  if (view?.layout !== next.layout || view?.role !== next.role) {
    map.clear();
    channels.replaceChildren();
    buses.replaceChildren();
  }
  view = next;
  const isDesk = view.role === 'desk';
  document.body.classList.toggle('desk', isDesk);
  el('pair').hidden = true;
  el('mixer').hidden = false;
  el('role').textContent = names[view.role];
  el('message').textContent = view.running ? 'Connected' : 'Engine off';
  el('led').classList.toggle('fp-led-g', view.running);
  buses.hidden = !view.controls.some(({ target }) => target.type !== 'strip');

  for (const item of view.controls) {
    const key = targetKey(item.target);
    const row = map.get(key) ?? makeRow(item, key);
    map.set(key, row);
    row.name.textContent = item.name;
    if (row.input) {
      row.input.disabled = !view.running;
      row.input.setAttribute('aria-label', `${item.name} ${isDesk ? 'fader' : 'monitor send'}`);
      if (!held.has(`${key}:level`) && !active.has(`${key}:level`) && !queue.has(`${key}:level`)) {
        row.input.value = item.db;
        row.input.setAttribute('aria-valuetext', dbLabel(item.db));
        row.value.textContent = dbLabel(item.db);
      }
    }
    if (row.mute) {
      row.mute.disabled = !view.running;
      row.mute.setAttribute('aria-label', `${item.name} mute`);
      if (!active.has(`${key}:mute`) && !queue.has(`${key}:mute`)) {
        setMute(row.mute, item.muted);
      }
    }
  }
}

function unpair() {
  events?.close();
  events = null;
  view = null;
  queue.clear();
  el('pair').hidden = false;
  el('mixer').hidden = true;
  el('role').textContent = 'Remote mixer';
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
  el('message').textContent = 'Disconnected';
  connectionLost = true;
  showError('Connection lost');
}

function write(key, target, kind, value) {
  if (!view?.running) return;
  const id = `${key}:${kind}`;
  queue.set(id, changeBody(view.layout, target, kind, value));
  if (!sending) void flush();
}

async function flush() {
  sending = true;
  while (queue.size) {
    const [id, change] = queue.entries().next().value;
    queue.delete(id);
    active.add(id);
    try {
      const response = await fetch('/api/control', {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(change),
      });
      if (response.status === 401) {
        unpair();
        showError('Pair again');
      } else if (response.status === 429) {
        if (!queue.has(id)) queue.set(id, change);
        await new Promise((resolve) => window.setTimeout(resolve, 40));
      } else if (!response.ok) {
        queue.clear();
        const errors = { 409: 'Mixer layout changed', 423: 'Fader in use' };
        showError(errors[response.status] ?? 'Control unavailable');
        if (view) {
          active.delete(id);
          render(view);
        }
        if (response.status === 409) void connect();
      } else {
        showError('');
      }
    } catch {
      queue.clear();
      disconnect();
    } finally {
      active.delete(id);
    }
  }
  sending = false;
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
void connect();
