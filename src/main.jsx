import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.jsx';
import LandingPage from './LandingPage.jsx';
import { ThemeProvider } from './contexts/ThemeContext.jsx';

// Detect Electron — no landing page needed in the desktop app
const isElectron = typeof window !== 'undefined' && navigator.userAgent.includes('Electron');
// Allow deep-linking to app via ?app or #app
const wantsApp   = window.location.search.includes('app') || window.location.hash === '#app';

function Root() {
  const [launched, setLaunched] = useState(isElectron || wantsApp);

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
