// Which screen a browser visit shows: the website at the root, or the studio desk.
// `?app` and `#app` are the old links; `/studio` is the address the website's key points at.

const STUDIO_PATH = /^\/studio(\/(index\.html)?)?$/;

/** True when this location should open the studio rather than the website. */
export function wantsStudio(location) {
  if (!location) return false;
  const { pathname = '/', search = '', hash = '' } = location;
  if (STUDIO_PATH.test(pathname)) return true;
  if (hash === '#app') return true;
  return new URLSearchParams(search).has('app');
}

/** Where the website's "Open studio" key goes. A plain link, so it works without JavaScript state. */
export const STUDIO_HREF = '/?app';
