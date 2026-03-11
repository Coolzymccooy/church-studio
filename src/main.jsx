import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.jsx';
import LandingPage from './LandingPage.jsx';
import { ThemeProvider } from './contexts/ThemeContext.jsx';

// Detect desktop app — Tauri sets window.__TAURI__, Electron sets electronAPI.isElectron
const isDesktopApp = Boolean(window.__TAURI__) || Boolean(window.electronAPI?.isElectron);
// Allow deep-linking to app via ?app or #app
const wantsApp = window.location.search.includes('app') || window.location.hash === '#app';

function Root() {
  const [launched, setLaunched] = useState(isDesktopApp || wantsApp);

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
    <Root />
  </StrictMode>,
);
