import React from 'react';

import Key from '../faceplate/Key.jsx';

/** Small confirm dialog layered over the OBS panel. `request` null = hidden. */
export default function ObsConfirmDialog({ request, onConfirm, onCancel }) {
  if (!request) return null;
  return (
    <>
      <div className="fp-scrim" aria-hidden="true" />
      <div className="fp-dialog-wrap">
        <div
          className="fp-dialog"
          style={{ width: 'min(360px, 100%)' }}
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="obs-confirm-title"
          aria-describedby="obs-confirm-message"
        >
          <div className="fp-dialog-hd">
            <span id="obs-confirm-title">{request.title}</span>
          </div>
          <div className="fp-dialog-bd">
            <p id="obs-confirm-message" className="fp-row" style={{ margin: 0, whiteSpace: 'normal' }}>
              {request.message}
            </p>
            <div className="fp-keys" style={{ justifyContent: 'flex-end' }}>
              <Key onClick={onCancel}>cancel</Key>
              <Key lit={request.danger ? 'amber' : null} onClick={onConfirm} autoFocus>
                {request.confirmLabel.toLowerCase()}
              </Key>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
