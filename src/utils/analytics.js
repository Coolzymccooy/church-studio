// In-app analytics tracker — stores events in localStorage
// No external services — all local to the user's device

const STORAGE_KEY = 'tiwaton_analytics';
const MAX_EVENTS = 500;

function getStore() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || '{"events":[],"totals":{}}');
  } catch {
    return { events: [], totals: {} };
  }
}

function saveStore(store) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
  } catch {}
}

export function trackEvent(name, data = {}) {
  const store = getStore();
  const event = { name, data, ts: Date.now() };
  store.events = [event, ...store.events].slice(0, MAX_EVENTS);
  store.totals[name] = (store.totals[name] || 0) + 1;
  saveStore(store);
}

export function getAnalytics() {
  return getStore();
}

export function getTotals() {
  return getStore().totals;
}

export function getRecentEvents(n = 20) {
  return getStore().events.slice(0, n);
}

export function clearAnalytics() {
  saveStore({ events: [], totals: {} });
}

// Session timer helper
let sessionStart = null;
export function startSession() {
  sessionStart = Date.now();
  trackEvent('session_start');
}
export function endSession() {
  if (sessionStart) {
    const duration = Math.round((Date.now() - sessionStart) / 1000);
    trackEvent('session_end', { durationSec: duration });
    sessionStart = null;
  }
}
