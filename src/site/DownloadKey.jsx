import { useEffect, useState } from 'react';
import { ChevronDown, Download } from 'lucide-react';
import { DOWNLOADS_MANIFEST_URL, detectPlatform, downloadOptions, parseDownloads } from './downloads.js';

const MANIFEST_TIMEOUT_MS = 8000;

/** downloads.json from the downloads host; null until it loads, and null if it never does. */
function useDownloadsManifest() {
  const [manifest, setManifest] = useState(null);
  useEffect(() => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), MANIFEST_TIMEOUT_MS);
    fetch(DOWNLOADS_MANIFEST_URL, { cache: 'no-cache', signal: controller.signal })
      .then((response) => (response.ok ? response.json() : null))
      .then((json) => setManifest(parseDownloads(json)))
      // Offline or blocked: the keys keep their stable latest URLs, just without version and size.
      .catch(() => setManifest(null))
      .finally(() => clearTimeout(timer));
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, []);
  return manifest;
}

const metaText = (option) => [option.version, option.size].filter(Boolean).join(' · ');

function OptionList({ options }) {
  return (
    <ul className="site-dl-list">
      {options.map((option) => (
        <li key={option.platform}>
          <a className="site-link" href={option.href}>
            <Download size={16} strokeWidth={2} aria-hidden="true" />
            {option.label} <span className="site-dl-kind">{option.kind}</span>
          </a>
          {option.size ? <span className="site-dl-meta">{option.size}</span> : null}
        </li>
      ))}
    </ul>
  );
}

/** The desktop download: one key for this visitor's OS, the other installers one click away. */
export default function DownloadKey() {
  const manifest = useDownloadsManifest();
  const [platform] = useState(() => (typeof navigator === 'undefined' ? null : detectPlatform(navigator)));
  const options = downloadOptions(manifest);
  const primary = options.find((option) => option.platform === platform);
  const others = options.filter((option) => option !== primary);

  if (!primary) {
    return (
      <div className="site-dl" id="download">
        <OptionList options={options} />
      </div>
    );
  }

  return (
    <div className="site-dl" id="download">
      <a className="site-dl-key" href={primary.href}>
        <Download size={18} strokeWidth={2} aria-hidden="true" />
        Download for {primary.label}
      </a>
      <div className="site-dl-line">
        {metaText(primary) ? <span className="site-dl-meta">{metaText(primary)}</span> : null}
        <details className="site-dl-other">
          <summary>
            Other platforms
            <ChevronDown size={16} strokeWidth={2} aria-hidden="true" />
          </summary>
          <OptionList options={others} />
        </details>
      </div>
    </div>
  );
}
