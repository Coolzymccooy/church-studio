/**
 * Copy text to the clipboard. The desktop webview can refuse the async clipboard, so fall back
 * to selecting a hidden text area and running the copy command. Resolves true when copied;
 * never throws, so a caller can show "copy failed" instead of looking dead.
 */
export async function copyText(text, { nav = globalThis.navigator, doc = globalThis.document } = {}) {
  if (!text) return false;
  try {
    if (nav?.clipboard?.writeText) {
      await nav.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Fall through to the text-area copy.
  }
  return copyViaTextArea(text, doc);
}

function copyViaTextArea(text, doc) {
  if (!doc?.createElement || !doc.body) return false;
  const area = doc.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.opacity = '0';
  area.style.pointerEvents = 'none';
  doc.body.appendChild(area);
  try {
    area.select();
    return Boolean(doc.execCommand('copy'));
  } catch {
    return false;
  } finally {
    doc.body.removeChild(area);
  }
}
