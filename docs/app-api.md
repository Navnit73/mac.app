# App API (usage stats and updates)

MyMacStorage talks to `https://mymacstorage.com/api/app/v1/`:

| Call | When |
| --- | --- |
| `POST heartbeat` | At launch (`event: "launch"`), then every 60 s (`"ping"`), 120 s after a failure. The response may carry an update. |
| `POST events` | Queued feature events, every 60 s, at 100 queued, and on quit (3 s limit). Failed batches are retried, up to 500 events. |
| `GET updates` | App menu → **Check for Updates…** |

Code: `crates/core/src/telemetry.rs` (payloads, validation, queue), `src-tauri/src/telemetry.rs` (HTTP, loops, commands), `src/state/updateStore.ts` + `src/components/UpdateDialog.tsx` (prompt).

## Building a release

Put the key in `.env` at the repo root (gitignored; `.env.example` shows the format):

```sh
MMS_APP_API_KEY=<same value as APP_API_KEY on the server>
MMS_UPDATE_CHANNEL=stable   # "beta" for tester builds
```

`src-tauri/build.rs` reads it at compile time; a variable set in the shell wins over the file. Then bump `"bundleVersion"` in `src-tauri/tauri.conf.json` (whole number, +1 every release) and `"version"` (e.g. 1.4.0), and run `npm run tauri build`.

Without `MMS_APP_API_KEY` the build makes no network calls, and **Check for Updates…** says update checks are not set up. Never commit the key.

Then sign, notarize and publish the DMG as in the website's integration guide, and add the release at `/navnit_admin/app`.

## What is sent

Heartbeat: random device id (stored in `~/Library/Application Support/MyMacStorage/settings.json`), per-launch session id, version, build, channel, macOS version, Mac model, CPU architecture, locale, and `licensed: false` (the server requires the field; the app has no licences).

Events (name → props):

| Event | Props |
| --- | --- |
| `<room>.opened` (`overview`, `space`, `find`, `duplicates`, `cleanup`, `applications`, `activity`) | none |
| `scan.started` | `roots` |
| `scan.completed` / `scan.cancelled` | `files`, `bytes`, `seconds` |
| `duplicates.deleted` | `count`, `bytes`, `clones` |
| `cleanup.completed` | `count`, `bytes` |
| `app.uninstalled` | `count`, `bytes` |
| `trash.emptied` | `batches` |
| `update.prompt_shown` / `update.download_clicked` | `build` (+ `required`) |

Never paths, file or app names, or emails. **Share Anonymous Usage Data** in the app menu turns events off; heartbeats continue so update offers still arrive.

## Update prompt

| Flags | Buttons |
| --- | --- |
| neither | Download, Later, Skip This Version |
| `critical` | Download, Later |
| `required` | Download, Quit (cannot be dismissed; shown again every launch) |

Download opens the DMG link in the browser; the user drags the new copy into Applications.
