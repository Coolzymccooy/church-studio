# Remote control and personal monitor mixes

## Listener and authentication

The desktop's Remote mixer rail starts and stops a dedicated HTTPS listener. It is off at startup. Listen on one explicitly chosen IPv4 address in RFC 1918, the tailnet CGNAT range, or loopback. The default port is 4475. Enter the hostname or IP that clients use, and select an existing PEM certificate chain and its private key. Clients use a hostname and CA they already trust. The TLS files remain on the desktop. Listener settings, codes and sessions are in memory and end when the listener stops.

Apply the Link server's exact Host allowlist on every request. For writes and pairing, also require the exact HTTPS origin and JSON, with a small request body. No CORS is enabled. The public entry page contains the pairing form. Mixer state and controls require a device session. Return no-store responses and block embedding and third-party scripts. Bound connection concurrency and handshake time.

The desktop operator chooses a role and generates a single-use, six-digit code. A code is valid for 120 seconds and eight failed attempts. A new code invalidates the previous one. The operator can cancel a code and revoke any device. Each accepted device receives a random 256-bit token in an HttpOnly, Secure, SameSite=Strict host-only cookie. Its server-side record expires after 12 hours or on revocation. Store token digests in memory, and keep credentials out of logs, URLs and error bodies.

## Permissions and controls

| Role | Snapshot and writable controls |
| --- | --- |
| Mon 1, Mon 2, Mon 3 | Each active input's send to the assigned monitor only. −90 to +10 dB. −90 is off. |
| Desk | Active input and output bus faders and mutes, plus FX returns and DCA masters when present. −90 to +10 dB. Mute is on or off. |

The phone uses 44 px touch controls. Desk mode supports tablet and desktop widths. Both use the faceplate finishes, keyboard controls, and an explicit connected state. Control commands validate IDs against the current layout, and write the same parameters that the local mixer uses. A stopped engine can show its settings but rejects remote writes. Pairing is possible with the engine stopped.

## API and timing

- GET / serves the entry page. CSS and JavaScript are served locally.
- POST /api/pair takes a device name and code and issues the scoped cookie.
- GET /api/state returns the scoped snapshot. POST /api/control takes one typed fader, mute, or monitor send change.
- Clients refresh scoped state every 40 ms while visible and immediately after a write. The desktop receives the existing mixer-state-changed event. A request applies to the shared mixer atomics as part of handling the write. The next callback block uses the new target, with the desk's existing gain ramps.
- Remote state carries a layout signature. Reject a write based on an outdated layout. The client refreshes on rejection and keeps its last acknowledged values.
- A new certificate or bind failure leaves the listener off. An expired or revoked cookie returns 401 and brings the client back to pairing.

Verify pairing, scope, denial paths, revocation, desk/monitor value propagation and same-host request latency. Measure command-to-audio timing on the church Wi-Fi and tailnet with real phones and tablets before relying on the 100 ms target during service.
