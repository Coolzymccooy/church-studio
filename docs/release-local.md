# Releasing and deploying without GitHub Actions

Everything CI does for a release can run from the two laptops. Publishing
goes through the GitHub Releases API (`gh`), which uses no Actions minutes,
and the web app deploys through Coolify.

| Job | Command | Where it runs |
|---|---|---|
| Desktop release, dry run | `npm run release:dry-run` | Mac + ajibowo |
| Desktop release | `npm run release:local` | Mac + ajibowo |
| Web deploy check | `npm run deploy:web -- --dry-run` | Mac |
| Web deploy | `npm run deploy:web` | Mac → Coolify |
| Script tests | `npm run test:release` | anywhere |

## One-time setup

1. **Updater signing key.** Put `tiwaton-updater.key` at
   `src-tauri/keys/tiwaton-updater.key` in the main checkout on the Mac
   (`~/Projects/church-studio`, git-ignored), or at `~/.tauri/tiwaton-updater.key`.
   You can also set `TAURI_SIGNING_PRIVATE_KEY` to a path or to the key text.
   The current key has no password. It is the key whose public half is in
   `src-tauri/tauri.conf.json` (key id `A9983B3A9BC583FC`). The release refuses
   to publish anything signed by a different key.
   The key stays on the Mac: ajibowo builds without updater signing and the Mac
   signs the installers after they come back.
2. **GitHub CLI:** `gh auth status` should show you logged in with `repo` scope.
3. **Coolify token:** `COOLIFY_API_TOKEN` (deploy permission) in `.env.local`.
   Never `source` that file: the token contains `|`, which the shell treats as a pipe.

## Desktop release

```bash
# 1. Bump the version in package.json, src-tauri/tauri.conf.json and src-tauri/Cargo.toml,
#    merge to master and push. The release builds the committed HEAD.
git checkout master && git pull

# 2. Dry run: builds, tests, signs and verifies both platforms, publishes nothing.
npm run release:dry-run

# 3. Release: the same steps, then asks you to type the tag to publish.
npm run release:local
```

What the script does, in order:

1. **Checks.** The three version fields agree, the tracked tree is clean, HEAD
   is on `origin/master`, and neither the tag nor the release exists yet.
   A dry run reports these as warnings; a release stops on them.
2. **Mac** (`~/tailnet-jobs/church-studio-release-mac`, a job worktree):
   `npm ci`, `tauri build --target universal-apple-darwin` (ad-hoc signed unless
   `APPLE_SIGNING_IDENTITY` is set), `npm test`, `cargo test --locked --all-targets`,
   then `scripts/verify-macos.py` (both architectures, entitlements, a 10-second launch).
3. **Windows**, at the same time on ajibowo (`C:\Users\segun\tailnet-jobs\church-studio-release`):
   the commit travels as a git bundle over scp, so GitHub isn't needed. Then
   `npm ci` and `tauri build` (NSIS + MSI), and the installers come back by scp.
4. **Signing:** the Mac signs the Windows installers, then every updater
   signature is checked with Node's own crypto against the key in `tauri.conf.json`,
   the same check the installed app makes.
5. **Files:** `latest.json` (same platform keys as v1.5.0), `SHA256SUMS.txt`,
   `release-notes.md` and `macos-verification.json` go to
   `~/tailnet-jobs/church-studio-artifacts/<tag>-<sha>/`, with logs in `logs/`.
6. **Publish** (not in a dry run): a draft release with every file, checked file
   by file against the local sizes, then published and marked latest. This is
   the same order as CI's "Publish verified release" job. The script then
   confirms that the updater URL serves the new version.

Options: `--skip-mac` / `--skip-windows` / `--skip-tests` (dry run only),
`--tag vX.Y.Z`, `--yes` (no prompt), `--clean-windows` (remove the ajibowo job
folder afterwards).

Without the real key, a dry run signs with a throwaway key and says so. That
proves the pipeline, but those files must never be published.

### Things to know

- **Linux is not built.** Neither laptop is x86_64 Linux. `latest.json`
  leaves out the `linux-*` entries, so Linux installs stop seeing updates until
  a Linux build is added. Mac and Windows are unaffected.
- **ajibowo Rust:** the `rustup` proxies in `%USERPROFILE%\.cargo\bin` point at
  a `rustup.exe` that is missing, so `cargo` fails from SSH. The build script
  puts `%USERPROFILE%\.rustup\toolchains\stable-*-pc-windows-msvc\bin` first on PATH
  instead. Reinstalling rustup on ajibowo fixes the proxies themselves.
- ajibowo needs at least 6 GB free; the script checks before it starts. The job
  folder is reused between releases, so its `target` cache only grows once.
- If SSH drops mid-build, the Windows build stops. Rerun; the cache makes it quicker.
- If a publish fails after the draft is created, the draft stays on GitHub for
  you to inspect or delete. Nothing goes live until every file has been checked.

## Web deploy (abike.tiwaton.co.uk)

Coolify app `abike-church-studio-web` builds `master` from GitHub on every push
through Coolify's own webhook; Actions are not involved. To deploy by hand:

```bash
npm run deploy:web -- --dry-run   # checks the app, the token, the last deployment and HTTPS
npm run deploy:web                # deploys origin/master, waits, then checks HTTPS
npm run deploy:web -- --force     # the same, rebuilding without Coolify's cache
```

Coolify builds what is on GitHub, not your local files, so push first. The
script refuses to deploy if the app's name, domain or branch has changed. It
never prints the token.
