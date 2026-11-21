// The mixer surface: strips, the LCD readout with fine adjust, pins, the mix master and the
// meter wiring. desk.js owns the connection and the write queue and calls in here.
import { attachFader, buildScale, buildTrack, showLevel } from './fader.js';
import { createMeters } from './meters.js';
import { clipUntil, CLIP_DB, DB_OFF, inMixDb, lcdReadout, nudge, orderStrips, peakLabel } from './mixmath.js';
import { dbLabel } from './protocol.js';

const MIX_NAMES = { desk: 'Desk', monitor1: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3' };
const KINDS = { bus: 'bus', return: 'fx', dca: 'dca' };
const STALE_METERS_MS = 600;
const el = (id) => document.getElementById(id);

export function targetKey(target) {
  return `${target.type}:${target.index ?? target.id}`;
}

function make(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function readPins(role) {
  try {
    const names = JSON.parse(localStorage.getItem(`tiwaton_remote_pins_${role}`) ?? '[]');
    return new Set(Array.isArray(names) ? names.filter((name) => typeof name === 'string') : []);
  } catch {
    return new Set();
  }
}

function savePins(role, pins) {
  try { localStorage.setItem(`tiwaton_remote_pins_${role}`, JSON.stringify([...pins])); } catch { /* optional storage */ }
}

/**
 * `onLevel(key, target, db)` and `onMute(key, target, muted)` send changes; `busy(id)` says a
 * change for that control id is still in flight, so incoming snapshots must not move it.
 */
export function createBoard({ onLevel, onMute, busy }) {
  const meters = createMeters();
  const strips = new Map();
  const held = new Set();
  let view = null;
  let pins = new Set();
  let selected = null;
  let reading = { strips: [], at: 0 };
  let clipLatch = 0;
  let personal = null;
  let shownPeak = '';
  let heldPeak = { db: -Infinity, until: 0 };
  let swept = false;

  const channels = el('channels');
  const buses = el('buses');
  const readout = el('readout');
  const fine = el('fine');
  const mute = el('mute-mix');
  buildScale(el('scale'));
  if (window.matchMedia('(min-width: 700px) and (min-height: 700px)').matches) {
    fine.hidden = false;
    readout.setAttribute('aria-expanded', 'true');
  }

  const isMon = () => view && view.role !== 'desk';
  const live = () => Boolean(view?.running);
  const fresh = () => performance.now() - reading.at < STALE_METERS_MS;

  function stripTargets(strip) {
    if (!fresh()) return [{ bar: -Infinity }];
    const { target } = strip.item;
    if (target.type === 'strip') {
      const tap = reading.strips[target.index];
      if (!isMon()) return [{ bar: tap ?? -Infinity, clip: tap >= CLIP_DB }];
      return [{ bar: inMixDb(tap, strip.db), ghost: tap ?? -Infinity, clip: tap >= CLIP_DB }];
    }
    const pair = target.type === 'bus' ? reading.buses?.[target.id] : null;
    return [{ bar: pair ? Math.max(pair[0], pair[1]) : -Infinity }];
  }

  function mixTargets() {
    if (!fresh()) return [{ bar: -Infinity }, { bar: -Infinity }];
    if (reading.mix) {
      const [pl, pr, rl, rr] = reading.mix;
      return [{ bar: rl, dot: pl }, { bar: rr, dot: pr }];
    }
    const main = reading.buses?.main ?? [-Infinity, -Infinity];
    return [{ bar: main[0] }, { bar: main[1] }];
  }

  meters.add(el('mix-meter'), { vertical: false, lanes: 2, read: mixTargets });
  meters.add(el('lcd-meter'), {
    vertical: false,
    read: () => (selected && strips.has(selected) ? stripTargets(strips.get(selected)) : [{ bar: -Infinity }]),
  });

  function setLevel(strip, db, send = true) {
    strip.db = db;
    showLevel(strip.track, db);
    strip.val.textContent = lcdReadout(db).trim();
    strip.el.classList.toggle('off', db <= DB_OFF);
    if (strip.key === selected) paintLcd();
    if (send) onLevel(strip.key, strip.item.target, db);
  }

  function select(key) {
    if (!strips.has(key) || key === selected) return;
    strips.get(selected)?.el.classList.remove('selected');
    strips.get(selected)?.scrib.setAttribute('aria-pressed', 'false');
    selected = key;
    const strip = strips.get(key);
    strip.el.classList.add('selected');
    strip.scrib.setAttribute('aria-pressed', 'true');
    readout.classList.remove('flip');
    void readout.offsetWidth;
    readout.classList.add('flip');
    paintLcd();
  }

  function paintLcd() {
    const strip = strips.get(selected);
    const text = lcdReadout(strip?.db ?? DB_OFF);
    el('lcd-no').textContent = strip?.no ?? '--';
    el('lcd-name').textContent = strip?.item.name ?? 'No channel';
    el('lcd-db').textContent = text;
    const muted = isMon() && Boolean(personal?.muted);
    el('lcd-mix').textContent = `${MIX_NAMES[view?.role] ?? ''}${muted ? ' muted' : ''}`;
    el('lcd-mix').classList.toggle('muted', muted);
    readout.setAttribute('aria-label', strip ? `${strip.item.name}, ${dbLabel(strip.db)}. Fine adjust` : 'No channel');
    const pinned = Boolean(strip && pins.has(strip.item.name));
    el('pin').setAttribute('aria-pressed', String(pinned));
  }

  function makeStrip(item, key) {
    const { target } = item;
    const node = make('div', 'strip');
    node.setAttribute('role', 'group');
    const scrib = make('button', 'scrib');
    scrib.type = 'button';
    scrib.setAttribute('aria-pressed', 'false');
    const no = target.type === 'strip' ? String(target.index + 1).padStart(2, '0') : KINDS[target.type] ?? '';
    const name = make('span', 'nm');
    scrib.append(make('span', 'no', no), name, make('span', 'pinled'));
    const fz = make('div', 'fz');
    const ladder = make('canvas', 'ladder');
    ladder.setAttribute('aria-hidden', 'true');
    const track = make('div', 'track');
    track.tabIndex = 0;
    track.setAttribute('role', 'slider');
    track.setAttribute('aria-orientation', 'vertical');
    track.setAttribute('aria-valuemin', String(DB_OFF));
    track.setAttribute('aria-valuemax', '10');
    buildTrack(track);
    fz.append(ladder, track);
    const val = make('output', 'val');
    node.append(scrib, fz, val);
    const strip = { key, item, el: node, scrib, name, track, val, no, db: item.db, mute: null, release: null };
    if (item.muted !== undefined) {
      strip.mute = make('button', 'key mute', 'Mute');
      strip.mute.type = 'button';
      strip.mute.onclick = () => {
        const muted = strip.mute.getAttribute('aria-pressed') !== 'true';
        strip.mute.setAttribute('aria-pressed', String(muted));
        onMute(key, strip.item.target, muted);
      };
      node.append(strip.mute);
    }
    scrib.onclick = () => select(key);
    attachFader(track, {
      get: () => strip.db,
      set: (db) => setLevel(strip, db),
      grab: (down) => (down ? held.add(`${key}:level`) : held.delete(`${key}:level`)),
      select: () => select(key),
      enabled: live,
    });
    strip.release = meters.add(ladder, { read: () => stripTargets(strip), order: strips.size });
    (target.type === 'strip' ? channels : buses).append(node);
    return strip;
  }

  function arrange() {
    const ordered = orderStrips([...strips.values()].filter((s) => s.item.target.type === 'strip')
      .map((s) => ({ name: s.item.name, strip: s })), pins);
    let lastPinned = null;
    for (const { strip } of ordered) {
      channels.append(strip.el);
      const pinned = pins.has(strip.item.name);
      strip.el.classList.toggle('pinned', pinned);
      strip.el.classList.remove('pin-end');
      if (pinned) lastPinned = strip;
    }
    if (lastPinned && ordered.length > pins.size) lastPinned.el.classList.add('pin-end');
  }

  function update(strip, item) {
    strip.item = item;
    strip.name.textContent = item.name;
    strip.el.setAttribute('aria-label', item.name);
    strip.track.setAttribute('aria-label', `${item.name} ${isMon() ? 'send' : 'fader'}`);
    strip.track.setAttribute('aria-disabled', String(!live()));
    const id = `${strip.key}:level`;
    if (!held.has(id) && !busy(id)) setLevel(strip, item.db, false);
    if (strip.mute) {
      strip.mute.disabled = !live();
      strip.mute.setAttribute('aria-label', `${item.name} mute`);
      if (!busy(`${strip.key}:mute`)) strip.mute.setAttribute('aria-pressed', String(Boolean(item.muted)));
    }
  }

  function paintMaster() {
    const desk = !isMon();
    mute.hidden = desk || !personal;
    mute.classList.add('big');
    mute.disabled = !live();
    el('mix-name').textContent = desk ? 'Main' : MIX_NAMES[view.role];
    if (personal && !busy(`${targetKey(personal.target)}:mute`)) mute.setAttribute('aria-pressed', String(Boolean(personal.muted)));
    mute.setAttribute('aria-label', `Mute ${MIX_NAMES[view.role]} mix`);
  }

  function firstKey() {
    const first = channels.firstElementChild ?? buses.firstElementChild;
    return [...strips.values()].find((strip) => strip.el === first)?.key;
  }

  function reset() {
    for (const strip of strips.values()) strip.release();
    strips.clear();
    held.clear();
    channels.replaceChildren();
    buses.replaceChildren();
    selected = null;
    personal = null;
    swept = false;
  }

  mute.onclick = () => {
    if (!personal) return;
    const muted = mute.getAttribute('aria-pressed') !== 'true';
    mute.setAttribute('aria-pressed', String(muted));
    personal = { ...personal, muted };
    paintLcd();
    onMute(targetKey(personal.target), personal.target, muted);
  };

  readout.onclick = () => {
    const open = fine.hidden;
    fine.hidden = !open;
    readout.setAttribute('aria-expanded', String(open));
  };

  fine.onclick = (event) => {
    const key = event.target.closest('button');
    const strip = strips.get(selected);
    if (!key || !strip) return;
    if (key.id === 'pin') {
      const name = strip.item.name;
      if (pins.has(name)) pins.delete(name); else pins.add(name);
      savePins(view.role, pins);
      arrange();
      paintLcd();
      strip.el.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
      return;
    }
    if (!live()) return;
    const step = key.dataset.step;
    const next = step === 'off' ? DB_OFF : step === 'unity' ? 0 : nudge(strip.db, Number(step));
    if (next !== strip.db) setLevel(strip, next);
  };

  return {
    reset,
    render(next) {
      const roleChanged = view?.role !== next.role;
      view = next;
      if (roleChanged) pins = readPins(next.role);
      document.body.classList.toggle('desk', next.role === 'desk');
      document.body.classList.add('mixing');
      el('mixer').classList.toggle('idle', !next.running);
      personal = next.controls.find(({ target }) => target.type === 'personal') ?? null;
      for (const item of next.controls) {
        if (item.target.type === 'personal') continue;
        const key = targetKey(item.target);
        const strip = strips.get(key) ?? makeStrip(item, key);
        strips.set(key, strip);
        update(strip, item);
      }
      buses.hidden = buses.childElementCount === 0;
      arrange();
      if (!selected || !strips.has(selected)) select(firstKey());
      paintMaster();
      paintLcd();
      if (!swept) {
        swept = true;
        meters.sweep();
      }
    },
    meters(data) {
      reading = { ...data, at: performance.now() };
      const peak = reading.mix ? Math.max(reading.mix[0], reading.mix[1]) : Math.max(...(reading.buses?.main ?? [-Infinity]));
      clipLatch = clipUntil(clipLatch, peak, performance.now());
      meters.wake();
    },
    /** Refresh the clip lamp and the held peak readout (desk.js calls this on a short timer). */
    tick() {
      const now = performance.now();
      el('clip').classList.toggle('on', now < clipLatch);
      const peak = fresh() ? mixTargets().reduce((top, lane) => Math.max(top, lane.dot ?? lane.bar), -Infinity) : -Infinity;
      if (peak >= heldPeak.db || now > heldPeak.until) heldPeak = { db: peak, until: now + 1000 };
      const label = peakLabel(heldPeak.db);
      if (label !== shownPeak) {
        shownPeak = label;
        el('mix-peak').textContent = label;
      }
    },
    refresh: () => meters.refresh(),
  };
}
