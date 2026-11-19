import { StrictMode, Suspense, lazy, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles/faceplate-fonts.js';
import './index.css';
import App from './App.jsx';
import LandingPage from './LandingPage.jsx';
import AppErrorBoundary from './components/AppErrorBoundary.jsx';
import { ThemeProvider } from './contexts/ThemeContext.jsx';
import { isDesktopApp } from './lib/platform.js';

// Detect desktop app — Tauri sets window.__TAURI__, Electron sets electronAPI.isElectron
// Allow deep-linking to app via ?app or #app
// Dev-only kit page for the faceplate primitives (?kit). The DEV check drops it from production builds.
const FaceplateKit = import.meta.env.DEV ? lazy(() => import('./components/faceplate/FaceplateKit.jsx')) : null;
const wantsKit = FaceplateKit !== null && new URLSearchParams(window.location.search).has('kit');

const wantsApp = window.location.search.includes('app') || window.location.hash === '#app';

function Root() {
  const [launched, setLaunched] = useState(isDesktopApp || wantsApp);

  if (wantsKit) {
    return <Suspense fallback={null}><FaceplateKit /></Suspense>;
  }

  const handleLaunch = () => {
    window.history.pushState({}, '', '?app');
    setLaunched(true);
  };

  if (!launched) {
    return <LandingPage onLaunch={handleLaunch} />;
  }

  return (
    <ThemeProvider>
      <App />
    </ThemeProvider>
  );
}

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <AppErrorBoundary>
      <Root />
    </AppErrorBoundary>
  </StrictMode>,
);
