# Remote control and personal monitor mixes

## Listener and certificates

The desktop's remote rail starts and stops a dedicated HTTPS listener. It is off at startup. Listen on one explicitly chosen IPv4 address in RFC 1918, the tailnet CGNAT range, or loopback. The default port is 4475. The last successful listener IP, port, hostname, and certificate file paths are saved in the desktop app data folder.

The desktop detects an active Tailscale IPv4 address and MagicDNS hostname and offers to request a trusted certificate using `tailscale cert`. It keeps the files in the desktop app data folder. Tailscale HTTPS must already be enabled. The hostname appears in public certificate transparency logs. Use the certificate command again before expiry and restart the listener to load the new certificate. See the [Tailscale HTTPS certificate instructions](https://tailscale.com/docs/how-to/set-up-https-certificates).

For a plain LAN, use an existing PEM certificate chain and private key for the exact hostname clients use. Establish trust with your organisation's CA on each device first. Compare the SHA-256 leaf fingerprint shown on the desktop and the pairing page after the browser has established a trusted connection. If the browser warns or the fingerprints differ, stop and check the hostname, certificate chain, and local CA with the operator. TLS files stay on the desktop.

## Pairing and sessions

Use the Link server's Host allowlist and require the exact HTTPS Origin for writes. No CORS. The public page contains only the pairing form and certificate fingerprint. State and controls require a device session. Use no-store responses and block embedding and third-party scripts. Allow at most 64 connections, eight per source IP, and reserve the last 16 slots for IPs seen with valid pairings. Every protected request still verifies its token.

The operator chooses a role and generates a single-use, six-digit code valid for 120 seconds. The desktop shows the active code and a QR. The QR supplies the code in a URL fragment that the browser removes after filling the form. Three failures from one IP enable exponential backoff from 2 to 60 seconds. Rotating a code does not reset that IP's backoff. The desktop reports the code as ready until it expires and shows backoff separately. New addresses can still pair if the failure table is full. A new failure at capacity evicts the oldest record.

Pairing issues a random 256-bit token as an HttpOnly, Secure, SameSite=Strict host-only cookie. The server stores only its digest and device metadata. The device expires after 12 hours unless it opted in to renewal and returns while the session is still valid. A renewal extends the same session by 12 hours. Listener stop and explicit revocation disconnect devices. Codes, sessions and renewal choices stay in memory.

## Permissions and controls

| Role | Snapshot and writable controls |
| --- | --- |
| Mon 1, Mon 2, Mon 3 | Each active input's send to the assigned monitor and the mute for that monitor output. −90 to +10 dB. −90 is off. Devices with the same role share that mix. |
| Desk | Active input and output bus faders and mutes, plus FX returns and DCA masters when present. −90 to +10 dB. Mute is on or off. |

A stopped engine shows settings and rejects remote writes. Pairing is available with the engine stopped.

## The desktop panel

The rail's remote key opens a popover that follows the flow: start listening, then the address with a copy key, then pick a role (mon 1, mon 2, mon 3, desk) and pair. An active code shows large with its role, a two-minute countdown and the QR. Paired devices list their name, a role chip, when they were last seen ("live" while connected) and a two-press revoke. Stop is two-press. Keys are lowercase; explanations sit in info tips.

## The phone and tablet page

The page is a personal monitor mixer in the faceplate language (graphite and bone, `--fp-*` tokens, glass windows dark in both finishes). Screenshots: `docs/screenshots/2026-11-21-phone-mix/`.

- **Strips.** One vertical fader per channel: 44 px cap (60 px on tablets), console taper with 0 dB three quarters up, a magnetic 0 dB detent and off at the bottom. Drags are relative, so a touch never makes the cap jump; sideways swipes scroll the bank. Double-tap gives 0 dB. Arrow keys step 0.5 dB, Page keys 3 dB, Home off, End +10. The desk role adds a mute key per strip and shows buses, FX returns and DCAs after a divider.
- **Meters.** A segmented ladder beside each fader, the desktop ladder's zones (green to −18, amber to −3, red) and ballistics, with a one-second peak hold. On a monitor role the lit bar is what the channel puts into this mix (its send tap plus the send level) and a faint bar shows signal at the tap the mix is not using. The mix meter shows RMS as the bar and peak as a dot with a hold tick, a held peak readout and a clip lamp latched for two seconds. All meters run on one animation-frame loop that idles when readings stop. A one-shot lamp-test sweep plays when a mixer loads. Reduced motion draws readings as they arrive, without smoothing or the sweep.
- **Readout.** A glass LCD near the thumbs shows the selected channel's number, name and level in a fixed-width readout, the mix name and its muted state, and a meter. Tapping it opens fine adjust: off, −1, −0.5, +0.5, +1, 0 dB and pin. Pinned channels (stored per role on the device) lead the bank.
- **Layout.** The link lamp (live, engine off, offline) and the mix name stay in the top bar. Portrait puts the readout and the mix master with its mute at the bottom; landscape puts them in a right-hand column. Tablets get wider strips, and the fine-adjust keys open by default.

## Live meters

GET /api/events also carries a read-only `meters` event, published on the same 60 ms cadence (about 16 Hz) only while that role has subscribers. Values are dBFS rounded to 0.1 dB.

| Role | `meters` payload |
| --- | --- |
| Mon 1, Mon 2, Mon 3 | `strips`: one level per strip at that mix's send tap (pre-fader, or post-fader where mon 1 follows the fader; −96 when the strip or its DCA is muted). `mix`: that monitor bus as peak L, peak R, RMS L, RMS R. Nothing about other buses, dynamics, automix or gates. |
| Desk | `strips`: each strip after its fader. `buses`: every bus as peak L, peak R. |

The desktop meters thread stores each `mixer-meters` reading for the listener and clears it when the engine stops; a reading older than 500 ms is not sent. A 32-strip monitor payload stays under 300 bytes. `node scripts/remote-mixer-preview.mjs` serves the page with a fake board and synthetic meters for layout work.

## Updates and failure behaviour

- POST /api/pair accepts a device name, code and optional renewal choice. POST /api/renew extends only an opted-in token.
- GET /api/state returns the initial role-scoped snapshot. GET /api/events opens a role-scoped SSE subscription. Changed snapshots are published on a shared 60 ms cadence. Connected clients do not repeatedly request mixer snapshots.
- POST /api/control takes a typed fader, mute, or monitor send change. Writes use the shared mixer controls. The next callback block sees the new target and uses the existing gain ramps. A per-device budget allows 30 writes/sec sustained with an eight-write burst. The client coalesces queued changes and retries a busy or rate-limited write with its newest value every 40 ms for up to two seconds.
- Auth guards are released before board work. Bounded blocking workers perform board calls. An engine transition or saturated worker pool returns 503. A pending remote fader change cannot overwrite a fader held by the local operator. The desktop renews the hold every 500 ms during a drag. If the webview stops renewing it, the hold expires after two seconds. Desktop mixer notifications are coalesced.
- The layout signature includes the input device identity and channel count, network slot and computer-pair layout, and ordered full NDI source names. A stale signature returns 409.
- Invalid listener configuration leaves the listener off. Revoked or expired sessions require pairing again.

Verify pairing, scope, denial paths, expiry, revocation, and two-client propagation over HTTPS. Measure command-to-audio timing and client sync with real devices on the intended church network before relying on the 100 ms target during a service.
