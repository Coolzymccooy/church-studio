# Download host: downloads.tiwaton.co.uk

Users download the desktop app from Tiwaton's own domain, never from GitHub. The installers live in
the Cloudflare R2 bucket `tiwaton-downloads`, served at `https://downloads.tiwaton.co.uk`.

## Why R2

R2 has no download (egress) fees and sits behind Cloudflare's CDN, the zone `tiwaton.co.uk` is
already on Cloudflare, and Lumina ships its updates the same way. One release is about 45 MB, so
the 10 GB free tier holds many versions. Serving the files from the Coolify server would need a
production volume and nginx change, uploads over SSH and no CDN.

## Layout

| Path | What | Cache |
|---|---|---|
| `studio/v<version>/<asset>` | every release file, never overwritten | 1 year, immutable |
| `studio/latest/mac.dmg`, `studio/latest/windows.exe` | stable links (saved under the real file name) | 5 minutes |
| `studio/latest.json` | updater manifest; URLs point at `studio/v<version>/` | revalidate every time |
| `studio/downloads.json` | version and sizes for the website's Download key | revalidate every time |

Never re-publish a version with different bytes: versioned files are cached for a year. Bump the
version instead.

## One-time setup (Cloudflare dashboard, ~5 minutes)

1. **R2 → Create bucket** `tiwaton-downloads` (location: automatic).
2. Bucket → **Settings → Custom Domains → Connect domain** `downloads.tiwaton.co.uk`.
   Cloudflare adds the DNS record itself. Leave the `r2.dev` public URL off.
3. Bucket → **Settings → CORS policy → Edit**, paste:
   ```json
   [{ "AllowedOrigins": ["https://abike.tiwaton.co.uk"], "AllowedMethods": ["GET", "HEAD"],
      "AllowedHeaders": ["*"], "MaxAgeSeconds": 3600 }]
   ```
   (The website reads `downloads.json` from the page; installers do not need CORS.)
4. **R2 → Manage API tokens → Create API token**: *Object Read & Write*, *Apply to specific
   buckets only* → `tiwaton-downloads`. Put the three values in the main checkout's `.env.local`
   (git-ignored), never in the repo:
   ```
   R2_ACCOUNT_ID=<32-character account id from the R2 overview page>
   R2_ACCESS_KEY_ID=<access key id>
   R2_SECRET_ACCESS_KEY=<secret access key>
   ```

## Publishing

`npm run release:local` puts every release here first (uploads, downloads each installer back and
compares it byte for byte, then switches `latest.json` and `downloads.json`), then on GitHub.

To put an already built release here on its own:

```bash
npm run release:downloads -- ~/tailnet-jobs/church-studio-artifacts/<tag>-<sha> --dry-run
npm run release:downloads -- ~/tailnet-jobs/church-studio-artifacts/<tag>-<sha>
```

A release made only through GitHub Actions (`tauri-build.yml`) does **not** reach this host: run
the second command on its files afterwards, or new installs and the website stay on the old version.

## Updates for installed apps

From 1.5.1 the app checks `https://downloads.tiwaton.co.uk/studio/latest.json` first and falls back
to the GitHub release only if that fails. Copies installed before that (1.5.0 and older) only know
the GitHub endpoint, so every release must also go to GitHub Releases until those have updated.
`release-local.mjs` does both.
