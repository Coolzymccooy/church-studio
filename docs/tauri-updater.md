# Tauri Auto Updates

TIWATON AI Studio checks for signed updates when a release build starts, from Tiwaton's own download
host first (see `docs/downloads-host.md`) and GitHub Releases as a fallback.

## What is configured

- Updater endpoints, in order: `https://downloads.tiwaton.co.uk/studio/latest.json`, then
  `https://github.com/Coolzymccooy/church-studio/releases/latest/download/latest.json` (fallback, and
  the only one 1.5.0 and older know)
- Public key: embedded in `src-tauri/tauri.conf.json`
- Release builds generate signed updater artifacts
- Debug builds (`tauri dev`) skip update checks

## GitHub secrets required

Set these repository secrets before creating release tags:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

For the current keypair, `TAURI_SIGNING_PRIVATE_KEY` should contain the contents of:

- `src-tauri/keys/tiwaton-updater.key`

The current local key was generated without a password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` can be left empty.

## Release flow

1. Push a tag like `v1.2.1` (or, with no Actions, run `npm run release:local`; see `docs/release-local.md`)
2. GitHub Actions builds the app and signs updater artifacts
3. The release must publish `latest.json` plus the signed installers
4. Installed desktop apps will prompt users to install the update on next launch

## Local build behavior

- `npm run tauri:build` will automatically use `src-tauri/keys/tiwaton-updater.key` if the env var is not already set
- The private key file is ignored by git
