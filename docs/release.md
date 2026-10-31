# Releasing MyMacStorage (direct download, not the App Store)

The app is sold and downloaded only from mymacstorage.com. macOS still requires it to be signed with a
**Developer ID Application** certificate and **notarized** by Apple, or customers' Macs refuse to open it.

## One-time setup

1. Join the Apple Developer Program (developer.apple.com/programs, $99/year).
2. Xcode → Settings → Accounts → your team → Manage Certificates → **+** → **Developer ID Application**.
   It is installed in your login Keychain. Check: `security find-identity -v -p codesigning`
   should list `Developer ID Application: <Name> (<TEAMID>)`.
3. Create an app-specific password at appleid.apple.com → Sign-In and Security → App-Specific Passwords.

## Each release

1. In `src-tauri/tauri.conf.json` raise `bundle.macOS.bundleVersion` by 1 (the server compares it)
   and set `version` (e.g. `1.0.0`).
2. Check `.env` has `MMS_APP_API_KEY` and `MMS_UPDATE_CHANNEL=stable` (see docs/app-api.md).
3. Build, sign and notarize (Tauri does all three when these are set):

   ```sh
   export APPLE_SIGNING_IDENTITY="Developer ID Application: <Name> (<TEAMID>)"
   export APPLE_ID="<apple id email>"
   export APPLE_PASSWORD="<app-specific password>"
   export APPLE_TEAM_ID="<TEAMID>"
   npm run tauri build -- --target universal-apple-darwin   # Apple Silicon + Intel
   ```

   Also set `signingIdentity` to the Developer ID name (or remove it so the env var is used) and `hardenedRuntime` to `true` in `tauri.conf.json`.
   Universal builds need `rustup target add x86_64-apple-darwin` once. Output:
   `target/universal-apple-darwin/release/bundle/dmg/`.

4. Check it: `spctl -a -vv -t install <the .dmg>` should say `accepted, source=Notarized Developer ID`.
5. Rename to `MyMacStorage-<version>.dmg`, upload it, note the size (`stat -f%z <file>`), and publish
   the release at `/navnit_admin/app`. Update `MAC_DOWNLOAD_URL` on the website.

## What the signing settings are for

- `hardenedRuntime: true`: required for notarization.
- `Entitlements.plist` → `com.apple.security.automation.apple-events`: Empty Trash asks Finder to empty
  the Trash; the hardened runtime blocks that without this entitlement.
- `Info.plist` → `NSAppleEventsUsageDescription`: the text macOS shows when it asks the user to allow it.
- The app is not sandboxed (a disk cleaner needs Full Disk Access), which Developer ID allows.
