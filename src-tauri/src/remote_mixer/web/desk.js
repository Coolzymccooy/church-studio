import { changeBody, dbLabel } from './protocol.js';

const el = (id) => document.getElementById(id);
const names = { desk: 'Desk', monitor1: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3' };
const channels = el('channels');
const buses = el('buses');
const map = new Map();
const queue = new Map();
const active = new Set();
let view = null;
let polling = false;
let sending = false;
let applyingPair = false;
let requested = 0;
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
  input.step = '0.5';
  input.oninput = () => {
    const db = Number(input.value);
    value.textContent = dbLabel(db);
    input.setAttribute('aria-valuetext', dbLabel(db));
    write(key, item.target, 'level', db);
  };
  row.append(readout, input);
  const mute = item.muted === undefined ? null : document.createElement('button');
  if (mute) {
    mute.type = 'button';
    mute.className = 'fp-key';
    mute.textContent = 'Mute';
    mute.onclick = () => write(key, item.target, 'mute', mute.getAttribute('aria-pressed') !== 'true');
    row.append(mute);
  }
  (item.target.type === 'strip' ? channels : buses).append(row);
  return { row, name, value, input, mute };
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
  buses.hidden = !isDesk;

  for (const item of view.controls) {
    const key = targetKey(item.target);
    const row = map.get(key) ?? makeRow(item, key);
    map.set(key, row);
    row.name.textContent = item.name;
    row.input.disabled = !view.running;
    row.input.setAttribute('aria-label', `${item.name} ${isDesk ? 'fader' : 'monitor send'}`);
    if (!active.has(`${key}:level`) && !queue.has(`${key}:level`)) {
      row.input.value = item.db;
      row.input.setAttribute('aria-valuetext', dbLabel(item.db));
      row.value.textContent = dbLabel(item.db);
    }
    if (row.mute) {
      row.mute.disabled = !view.running;
      row.mute.setAttribute('aria-label', `${item.name} mute`);
      if (!active.has(`${key}:mute`) && !queue.has(`${key}:mute`)) {
        row.mute.setAttribute('aria-pressed', item.muted);
        row.mute.classList.toggle('fp-on-amber', item.muted);
      }
    }
  }
}

function unpair() {
  view = null;
  queue.clear();
  el('pair').hidden = false;
  el('mixer').hidden = true;
  el('role').textContent = 'Remote mixer';
}

async function refresh() {
  if (polling || applyingPair || document.hidden) return;
  polling = true;
  const generation = requested;
  try {
    const response = await fetch('/api/state');
    if (generation !== requested || applyingPair) return;
    if (response.status === 401) {
      if (view) showError('Pair again');
      unpair();
    } else if (response.ok) {
      const next = await response.json();
      if (generation === requested && !applyingPair) {
        if (connectionLost) showError('');
        connectionLost = false;
        render(next);
      }
    } else {
      disconnect();
    }
  } catch {
    disconnect();
  } finally {
    polling = false;
  }
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
  requested += 1;
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
      } else if (!response.ok) {
        queue.clear();
        showError(response.status === 409 ? 'Mixer layout changed' : 'Control unavailable');
      } else {
        showError('');
      }
    } catch {
      queue.clear();
      disconnect();
    } finally {
      active.delete(id);
      requested += 1;
    }
  }
  sending = false;
  void refresh();
}

el('pair').onsubmit = async (event) => {
  event.preventDefault();
  if (applyingPair) return;
  applyingPair = true;
  showError('');
  const button = event.currentTarget.querySelector('button');
  button.disabled = true;
  const code = el('code').value;
  el('code').value = '';
  try {
    const response = await fetch('/api/pair', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, name: el('name').value.trim() }),
    });
    if (!response.ok) showError(response.status === 401 ? 'Code invalid or expired' : 'Pairing failed');
  } catch {
    showError('Connection lost');
  } finally {
    applyingPair = false;
    button.disabled = false;
  }
  void refresh();
};

void refresh();
setInterval(refresh, 40);
document.addEventListener('visibilitychange', refresh);
