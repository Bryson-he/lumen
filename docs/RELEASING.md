# Making a release

Lumen updates itself from this repository's GitHub releases. Each release holds the installer, its signature, and `latest.json`, which tells installed copies what the newest version is.

## The signing key

Updates are signed with a key that is **not** in the repository. It lives on the release machine at `%USERPROFILE%\.tauri\lumen-updater.key` (its public half is in `apps/settings/src-tauri/tauri.conf.json`). Keep a backup somewhere safe. If it is lost, installed copies can no longer be updated automatically and people would have to download a new installer by hand. This key is separate from Windows code signing, which Lumen does not have.

## Steps

1. Set the new version: `node scripts/set-version.mjs 0.1.1`
2. Run the tests: `cargo test --workspace`
3. Build, with the key set (PowerShell):

   ```
   $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\lumen-updater.key" -Raw
   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
   cd apps/settings; pnpm tauri build; cd ../..
   ```

4. Write `latest.json`: `node scripts/make-latest.mjs 0.1.1 "What changed."`
5. Commit, tag and push: `git commit -am "Lumen 0.1.1"`, `git tag v0.1.1`, `git push origin main v0.1.1`
6. Publish:

   ```
   gh release create v0.1.1 target/release/bundle/nsis/Lumen_0.1.1_x64-setup.exe target/release/bundle/nsis/Lumen_0.1.1_x64-setup.exe.sig target/release/bundle/nsis/latest.json --title "Lumen 0.1.1" --notes "What changed."
   ```

Installed copies see the new version the next time they check (once a day when the app is opened, or "Check now" under Settings).

## Adding to the gallery

Accepted submissions go in `gallery/` with an entry in `gallery/gallery.json` (see `gallery/README.md`). Pushing to `main` is all it takes: the online gallery is read straight from the repository, and no release is needed.
