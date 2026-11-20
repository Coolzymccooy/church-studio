# Remote control and personal monitor mixes

## Listener and certificates

The desktop's remote rail starts and stops a dedicated HTTPS listener. It is off at startup. Listen on one explicitly chosen IPv4 address in RFC 1918, the tailnet CGNAT range, or loopback. The default port is 4475. The last successful listener IP, port, hostname, and certificate file paths are saved in the desktop app data folder.

The desktop detects an active Tailscale IPv4 address and MagicDNS hostname and offers to request a trusted certificate using `tailscale cert`. It keeps the files in the desktop app data folder. Tailscale HTTPS must already be enabled. The hostname appears in public certificate transparency logs. Use the certificate command again before expiry and restart the listener to load the new certificate. See the [Tailscale HTTPS certificate instructions](https://tailscale.com/docs/how-to/set-up-https-certificates).

For a plain LAN, use an existing PEM certificate chain and private key for the exact hostname clients use. Establish trust with your organisation's CA on each device first. Compare the SHA-256 leaf fingerprint shown on the desktop and the pairing page after the browser has established a trusted connection. If the browser warns or the fingerprints differ, stop and check the hostname, certificate chain, and local CA with the operator. TLS files stay on the desktop.

## Pairing and sessions

Use the Link server's Host allowlist and require the exact HTTPS Origin for writes. No CORS. The public page contains only the pairing form and certificate fingerprint. State and controls require a device session. Use no-store responses and block embedding and third-party scripts. Allow at most 64 connections, four per source IP, and reserve the last 16 slots for IPs seen with valid pairings. Every protected request still verifies its token.

The operator chooses a role and generates a single-use, six-digit code valid for 120 seconds. The desktop shows the active code and a QR. The QR supplies the code in a URL fragment that the browser removes after filling the form. Three failures from one IP enable exponential backoff from 2 to 60 seconds. Rotating a code does not reset that IP's backoff. The desktop reports code expiry and any IP currently in backoff. An active code can still pair a device from another IP.

Pairing issues a random 256-bit token as an HttpOnly, Secure, SameSite=Strict host-only cookie. The server stores only its digest and device metadata. The device expires after 12 hours unless it opted in to renewal and returns while the session is still valid. A renewal extends the same session by 12 hours. Listener stop and explicit revocation disconnect devices. Codes, sessions and renewal choices stay in memory.

## Permissions and controls

| Role | Snapshot and writable controls |
| --- | --- |
| Mon 1, Mon 2, Mon 3 | Each active input's send to the assigned monitor and the mute for that monitor output. −90 to +10 dB. −90 is off. Devices with the same role share that mix. |
| Desk | Active input and output bus faders and mutes, plus FX returns and DCA masters when present. −90 to +10 dB. Mute is on or off. |

The phone uses 44 px touch controls. The desk uses 196 px fader travel in 0.1 dB steps. Both offer the graphite and bone finishes and keyboard controls. A stopped engine shows settings and rejects remote writes. Pairing is available with the engine stopped.

## Updates and failure behaviour

- POST /api/pair accepts a device name, code and optional renewal choice. POST /api/renew extends only an opted-in token.
- GET /api/state returns the initial role-scoped snapshot. GET /api/events opens a role-scoped SSE subscription. Changed snapshots are published on a shared 60 ms cadence. Connected clients do not repeatedly request mixer snapshots.
- POST /api/control takes a typed fader, mute, or monitor send change. Writes use the shared mixer controls. The next callback block sees the new target and uses the existing gain ramps. A per-device budget allows 30 writes/sec sustained with an eight-write burst. The client coalesces queued changes and retries a rate-limited write with its newest value.
- Auth guards are released before board work. Bounded blocking workers perform board calls. An engine transition or saturated worker pool returns 503. A pending remote fader change cannot overwrite a channel or bus fader held by the local operator. Desktop mixer notifications are coalesced.
- The layout signature includes the input device identity and channel count, network slot and computer-pair layout, and ordered full NDI source names. A stale signature returns 409.
- Invalid listener configuration leaves the listener off. Revoked or expired sessions require pairing again.

Verify pairing, scope, denial paths, expiry, revocation, and two-client propagation over HTTPS. Measure command-to-audio timing and client sync with real devices on the intended church network before relying on the 100 ms target during a service.
