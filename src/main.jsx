import { StrictMode, Suspense, lazy } from 'react';
import { createRoot } from 'react-dom/client';
import './styles/faceplate-fonts.js';
import './index.css';
import App from './App.jsx';
import SitePage from './site/SitePage.jsx';
import { wantsStudio } from './site/route.js';
import AppErrorBoundary from './components/AppErrorBoundary.jsx';
import { ThemeProvider } from './contexts/ThemeContext.jsx';
import { isDesktopApp } from './lib/platform.js';

// The desktop app always opens the studio. In a browser the root shows the website, and the studio
// opens at /?app, #app or /studio (src/site/route.js).
// Dev-only kit page for the faceplate primitives (?kit). The DEV check drops it from production builds.
const FaceplateKit = import.meta.env.DEV ? lazy(() => import('./components/faceplate/FaceplateKit.jsx')) : null;
const wantsKit = FaceplateKit !== null && new URLSearchParams(window.location.search).has('kit');

const showStudio = isDesktopApp || wantsStudio(window.location);

function Root() {
  if (wantsKit) {
    return <Suspense fallback={null}><FaceplateKit /></Suspense>;
  }

  if (!showStudio) {
    return <SitePage />;
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
