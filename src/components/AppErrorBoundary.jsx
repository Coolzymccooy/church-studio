import React from 'react';

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
        <div
          className="min-h-screen flex items-center justify-center px-6"
          style={{ background: '#030710', color: '#e2e8f0' }}
        >
          <div
            className="w-full max-w-xl rounded-2xl border px-6 py-7 shadow-2xl"
            style={{
              background: '#0D1428',
              borderColor: 'rgba(245, 158, 11, 0.25)',
            }}
          >
            <p
              className="text-[11px] font-bold tracking-[0.2em] uppercase"
              style={{ color: '#f59e0b' }}
            >
              Studio Recovery
            </p>
            <h1 className="mt-3 text-2xl font-bold text-white">
              The app hit a render error.
            </h1>
            <p className="mt-2 text-sm text-slate-400">
              The window stayed alive so you are not left with a blank desktop shell.
              Reload the studio and retry the last action.
            </p>
            <div className="mt-5 flex gap-3">
              <button
                type="button"
                onClick={this.handleReload}
                className="rounded-lg px-4 py-2 text-sm font-semibold text-black"
                style={{ background: '#f59e0b' }}
              >
                Reload Studio
              </button>
            </div>
            {import.meta.env.DEV && (
              <pre className="mt-5 overflow-auto rounded-xl border border-slate-800 bg-[#050A1C] p-3 text-xs text-rose-300">
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
