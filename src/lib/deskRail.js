/** Logic for the desk's icon rails: alert rules, the bell lamp, and popover placement. */

const EDGE = 8;
const GAP = 8;

/**
 * Alerts the operator should see, most serious first. Each is { id, tone: 'r' | 'a', text, tip,
 * action? } where action is 'setup' | 'ndi-tools' | 'monitor'. Only what needs attention: no "all good" rows.
 */
export function deskAlerts({
  engineError = null, ndi = null, broadcastSet = true, isLive = false,
  droppedSamples = 0, cpuPct = null, lufsST = null, target = null, computerMutedUnheard = false,
} = {}) {
  const alerts = [];
  if (engineError) {
    alerts.push({ id: 'engine', tone: 'r', text: 'engine failed', tip: String(engineError) });
  }
  if (isLive && typeof lufsST === 'number' && typeof target === 'number' && lufsST > target + 3) {
    alerts.push({ id: 'loud', tone: 'r', text: 'stream too loud', tip: `Short-term ${lufsST.toFixed(1)} LUFS against a ${target} target. Lower the Stream bus or turn the limiter on.` });
  }
  if (isLive && computerMutedUnheard) {
    alerts.push({ id: 'computer-muted', tone: 'a', text: 'computer audio muted at speakers', tip: 'While computer audio is captured, the Mac mutes those apps at their normal output, so the music only reaches you through the desk. Turn the monitor on to hear it.', action: 'monitor' });
  }
  if (isLive && droppedSamples > 0) {
    alerts.push({ id: 'drops', tone: 'a', text: `${droppedSamples} dropped samples`, tip: 'Audio is breaking up. Raise the buffer in setup or close other apps.' });
  }
  if (isLive && typeof cpuPct === 'number' && cpuPct > 90) {
    alerts.push({ id: 'cpu', tone: 'a', text: `cpu ${Math.round(cpuPct)}%`, tip: 'The audio engine is close to its limit. A release build or a larger buffer helps.' });
  }
  if (!broadcastSet) {
    alerts.push({ id: 'broadcast', tone: 'a', text: 'broadcast not set', tip: 'Nothing is chosen for the stream output. Choose a device in setup, or send the Stream bus over NDI.', action: 'setup' });
  }
  if (ndi && ndi.checked && !ndi.available) {
    alerts.push({ id: 'ndi', tone: 'a', text: 'NDI runtime missing', tip: 'The NDI runtime that ships with the app did not load. Reinstall the app, or install NDI Tools.', action: 'ndi-tools' });
  }
  return alerts;
}

/** The bell key's LED: red if any alert is red, amber if any is amber, else off. */
export function alertLamp(alerts) {
  if (alerts.some((a) => a.tone === 'r')) return 'r';
  if (alerts.length > 0) return 'a';
  return 'off';
}

/**
 * Fixed-position placement of a popover beside its rail key: to the left of a right-hand rail, to
 * the right of a left-hand one, aligned with the key's top, kept inside the viewport.
 */
export function popPosition(keyRect, popSize, viewport, side) {
  const left = side === 'right'
    ? keyRect.left - GAP - popSize.w
    : keyRect.right + GAP;
  const maxTop = Math.max(EDGE, viewport.h - popSize.h - EDGE);
  return {
    top: Math.round(Math.min(maxTop, Math.max(EDGE, keyRect.top))),
    left: Math.round(Math.min(viewport.w - popSize.w - EDGE, Math.max(EDGE, left))),
  };
}

/** The tallest a popover may be from `top` without leaving the viewport. */
export function popMaxHeight(top, viewportHeight) {
  return Math.max(120, Math.floor(viewportHeight - top - EDGE));
}
