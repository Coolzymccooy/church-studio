import { createContext, useContext, useEffect, useState } from 'react';
import { FINISHES, nextFinish, readFinish } from '../lib/finish.js';

const ThemeContext = createContext(null);
const STORAGE_KEY = 'tiwaton_finish';

function loadFinish() {
  try {
    return readFinish(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return readFinish(null);
  }
}

/**
 * Holds the desk finish (graphite, bone or auto) and sets `data-finish` on <html>.
 * Replaces the old accent-colour themes (decision of 19 Nov 2026).
 */
export function ThemeProvider({ children }) {
  const [finish, setFinish] = useState(loadFinish);

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute('data-finish', finish);
    root.removeAttribute('data-theme');
    try {
      window.localStorage.setItem(STORAGE_KEY, finish);
    } catch {
      // Storage can be blocked; the finish still applies for this session.
    }
  }, [finish]);

  const cycleFinish = () => setFinish((f) => nextFinish(f));

  return (
    <ThemeContext.Provider value={{ finish, setFinish, cycleFinish, finishes: FINISHES }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  return useContext(ThemeContext);
}
