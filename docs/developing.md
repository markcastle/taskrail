# Developing Task Rail

Everything you need to run it from source, test it, find your way around the code, and cut a release. For why the code is shaped the way it is, read [architecture.md](architecture.md).

## Prerequisites

- A stable Rust toolchain, 1.80 or later. [rustup.rs](https://rustup.rs) is the easy way.
- Node 22 or later.
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. On Windows that is the Visual Studio C++ build tools and WebView2, which Windows 11 already has. On macOS it is Xcode's command line tools.

## Running it

```bash
npm install
```

```bash
npm run dev
```

That starts the Vite dev server and launches the rail with hot reload for the frontend. Changes to Rust rebuild and relaunch.

The first line in the log says which commit you are running, so you can tell a stale build from a fresh one:

```
Task Rail 0.1.0 (de65239) starting
```

Linux cannot reserve screen space, so the rail falls back to an always-on-top window at the edge, which is enough to work on the UI.

### Frontend only

To hack on the React side in a browser with sample data and no Rust involved:

```bash
npm run ui:dev
```

Open the URL it prints. Add `#settings` to see the settings pages on their own. The browser build keeps its state in `localStorage`; clear site data for a fresh start.

### A fake tracker to sync against

The app binary doubles as an MCP server with seven tickets on two boards:

```bash
cargo run -p taskrail-app -- --fake-mcp-server
```

Settings → Connectors → **Add demo tracker** connects to it in-process, so you rarely need to run it by hand.

## Tests

The product rules are specified in the Rust tests. Run them all:

```bash
cargo test --workspace
```

The frontend is driven through its gestures in a headless browser, the way CI does it. Once per machine:

```bash
npx playwright install chromium
```

Then, with nothing else on port 5173:

```bash
npm run ui:test
```

Before you push:

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && npm run lint
```

CI runs the core crates on Linux, the browser test, and the full build and test on Windows and macOS.

## Where things are

| Path | What lives there |
| --- | --- |
| `crates/core` | The domain model, the ports, and the rules for what the rail shows. No I/O, fully tested. Start here. |
| `crates/platform` | Desktop docking. The Windows AppBar and the macOS floating panel behind one trait. |
| `crates/store-sqlite` | Where items and settings live between runs. One SQLite file, bundled, nothing to install. |
| `crates/connector-api` | The contract every connector implements, and the small remote item model they all translate into. |
| `crates/connector-mcp` | The generic connector: any MCP server described by a Connector Profile. Discovery, sign-in, and the fake tracker. |
| `crates/sync` | Merging what a connector returns into the rail without disturbing what the user arranged. |
| `crates/llm` | One small door to a language model, used only by discovery. Anthropic natively; OpenAI and OpenRouter through the chat completions shape. |
| `src-tauri` | The Tauri shell: the window, the commands the UI calls, the dock, the tray, the hotkey, secrets, logs. |
| `ui` | The React frontend: the rail and the settings pages. |
| `scripts` | The browser test runner and the release script. |
| `docs` | What you are reading. |

Dependencies point inward. `src-tauri` depends on the crates; the crates depend only on `core`; `core` depends on nothing of ours. If a change in one crate forces a change in `core`, stop and ask whether it belongs there.

## Where the app keeps things

| What | Windows | macOS |
| --- | --- | --- |
| Tasks and settings | `%APPDATA%\com.markcastle.taskrail\rail.sqlite` | `~/Library/Application Support/com.markcastle.taskrail/rail.sqlite` |
| Logs | `%LOCALAPPDATA%\com.markcastle.taskrail\logs\taskrail.log` | `~/Library/Logs/com.markcastle.taskrail/taskrail.log` |
| API keys and sign-in tokens | Windows Credential Manager | Keychain |

Delete the SQLite file for a fresh start. The log folder opens from ⚙️ → Logs → **Open folder**, and from the tray.

Secrets are never written to the database or to a profile. A profile only names the credential store entry it needs.

## Cutting a release

Pick the number first. A patch (0.2.0 to 0.2.1) is for fixes and small adjustments. A minor (0.2.x to 0.3.0) is for anything a person would notice as new: a feature, a redesign, a new connector ability. Leaving 0.x behind is a judgement about stability, not a feature count.

Then make sure [CHANGELOG.md](../CHANGELOG.md) says what the release holds. Every change a person would notice goes under **Unreleased** as it lands, in one of **Added**, **Changed**, **Fixed** or **Removed**, written for the person using the app: what they will see, not which function moved. The release command refuses to run while that section is empty, because those notes become the GitHub Release and what the app shows as **Release notes** when it offers the update.

Then two commands:

```bash
npm run release -- 0.1.1
```

```bash
git push --follow-tags
```

The first dates the **Unreleased** section of the changelog as `0.1.1` and opens a fresh one, sets the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json`, updates `Cargo.lock`, commits and tags `v0.1.1`. The push starts the Release workflow, which:

1. Checks the tag matches the version in the config, so a mistyped tag fails fast.
2. Builds a Windows installer (NSIS, per-user, no admin prompt) and a universal macOS disk image.
3. Publishes a GitHub Release with both attached and the changelog section as the notes.

`install.ps1` and `install.sh` at the repo root read `releases/latest`, so the moment the workflow finishes the one-line install picks up the new version.

### How the app updates itself

Every release is signed with the updater key, and the workflow writes a `latest.json` manifest into the release. The running app reads that manifest twenty seconds after start and every hour, verifies the signature with the public key in `src-tauri/tauri.conf.json`, and either installs the update at once (the **Update automatically** switch in Settings) or shows an "Install now" banner on the rail. Settings also has **Check for updates**.

Two repository secrets make the signing work, and a release built without them has no signatures, so running apps will refuse it:

| Secret | What it holds |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The contents of the updater private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password |

If the key is ever lost, generate a new pair with `npx tauri signer generate -w taskrail.key`, put the new public key in `tauri.conf.json`, update the secrets, and ship a release. Apps already installed will not accept updates signed with the new key, so that release has to be installed by hand once.

The builds are not code-signed for the operating system. When a Windows certificate and an Apple Developer ID are available they go in as repository secrets and the workflow picks them up; until then the install scripts clear the first-run warnings.

## Conventions

- For anything a person sees or touches, ask what Apple would do. Fewer controls, better defaults, feedback for every action, nothing that needs explaining. If a feature needs a paragraph of help text, the design is not finished.
- Commit messages say what changed and why, in sentences. The first line is the headline.
- Tests describe behaviour: `a_profile_can_fan_out_over_boards`, not `test_fan_out`.
- Comments explain why, never what. If a what needs a comment, rename the thing.
- The UI never calls the model. Only discovery does, through `crates/llm`.
- Never create a second window on Windows. A second webview next to a docked AppBar hangs the app. Settings render inside the rail, which widens to make room.
