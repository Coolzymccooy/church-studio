import React from 'react';

import Key from './faceplate/Key.jsx';
import Lamp from './faceplate/Lamp.jsx';

export default class AppErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error) {
    return { error };
  }

  componentDidCatch(error, errorInfo) {
    console.error('[TIWATON] Unhandled render error', error, errorInfo);
  }

  handleReload = () => {
    if (typeof window !== 'undefined') {
      window.location.reload();
    }
  };

  render() {
    const { error } = this.state;

    if (error) {
      return (
        <div className="fp-desk" style={{ minHeight: '100dvh', display: 'grid', placeItems: 'center', padding: 16 }}>
          <div className="fp-card" role="alert" style={{ maxWidth: 520 }}>
            <Lamp tone="r">render error</Lamp>
            <h3>The studio needs a reload</h3>
            <Key lit="green" className="fp-key-tall" onClick={this.handleReload}>reload studio</Key>
            {import.meta.env.DEV && (
              <pre className="fp-glass" style={{ margin: 0, padding: 8, maxWidth: '100%', overflow: 'auto', textAlign: 'left', fontSize: 'var(--fp-text-xs)' }}>
                {String(error?.stack || error)}
              </pre>
            )}
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
