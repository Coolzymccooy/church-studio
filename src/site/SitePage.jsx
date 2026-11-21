import { ArrowRight } from 'lucide-react';
import DownloadKey from './DownloadKey.jsx';
import SiteConsole from './SiteConsole.jsx';
import { STUDIO_HREF } from './route.js';
import { DOWNLOAD_HREF, FEATURES, HERO } from './siteContent.js';
import './site.css';

const YEAR = new Date().getFullYear();

function Brand() {
  return (
    <a className="site-brand" href="/" aria-label="TIWATON AI Studio, home">
      <b>tiwaton</b>
      <span>ai studio</span>
    </a>
  );
}

function TopBar() {
  return (
    <header className="site-bar">
      <div className="site-wrap site-bar-row">
        <Brand />
        <nav className="site-nav" aria-label="Main">
          <a className="site-link" href={DOWNLOAD_HREF}>Download</a>
          <a className="site-key" href={STUDIO_HREF}>{HERO.cta}</a>
        </nav>
      </div>
    </header>
  );
}

function Hero() {
  return (
    <section className="site-wrap site-hero" aria-labelledby="site-title">
      <div className="site-hero-text">
        <h1 id="site-title">{HERO.title}</h1>
        <p className="site-lead">{HERO.line}</p>
        <div className="site-actions">
          <a className="site-key site-key-lg" href={STUDIO_HREF}>
            {HERO.cta}
            <ArrowRight size={18} strokeWidth={2} aria-hidden="true" />
          </a>
          <DownloadKey />
        </div>
      </div>
      <SiteConsole />
    </section>
  );
}

function Features() {
  return (
    <section className="site-wrap site-features" aria-labelledby="site-features-title">
      <h2 id="site-features-title">On the desk</h2>
      <ol className="site-spec">
        {FEATURES.map((f, i) => (
          <li key={f.title}>
            <span className="site-spec-no" aria-hidden="true">{String(i + 1).padStart(2, '0')}</span>
            <h3>{f.title}</h3>
            <p>{f.line}</p>
          </li>
        ))}
      </ol>
    </section>
  );
}

function Footer() {
  return (
    <footer className="site-foot">
      <div className="site-wrap site-foot-row">
        <Brand />
        <nav className="site-foot-links" aria-label="Footer">
          <a className="site-link" href={STUDIO_HREF}>{HERO.cta}</a>
          <a className="site-link" href={DOWNLOAD_HREF}>Download</a>
        </nav>
        <small>© {YEAR} Tiwaton Ltd</small>
      </div>
    </footer>
  );
}

/** The public website at the root of the web build. The studio itself opens at /?app or /studio. */
export default function SitePage() {
  return (
    <div className="site">
      <TopBar />
      <main>
        <Hero />
        <Features />
      </main>
      <Footer />
    </div>
  );
}
