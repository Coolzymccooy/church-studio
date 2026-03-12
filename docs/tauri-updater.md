# Tauri Auto Updates

TIWATON AI Studio now checks for signed updates from GitHub Releases when a release build starts.

## What is configured

- Updater endpoint: `https://github.com/Coolzymccooy/church-studio/releases/latest/download/latest.json`
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

1. Push a tag like `v1.2.1`
2. GitHub Actions builds the app and signs updater artifacts
3. The release must publish `latest.json` plus the signed installers
4. Installed desktop apps will prompt users to install the update on next launch

## Local build behavior

- `npm run tauri:build` will automatically use `src-tauri/keys/tiwaton-updater.key` if the env var is not already set
- The private key file is ignored by git
