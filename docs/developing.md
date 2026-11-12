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

### Retaking the README's screenshots

The pictures in the README come from the browser build, not from a desktop, so anyone can retake them. With `npm run ui:dev` running:

```bash
node scripts/readme-shots.cjs docs/images
```

It writes the rail in light and dark, a task opened, the pin both ways, and two Settings pages, at a fixed clock so the result does not depend on when it is run.

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

The work of a release is done before it, in [CHANGELOG.md](../CHANGELOG.md). Every change a person would notice goes under **Unreleased** as it lands, in one of **Added**, **Changed**, **Fixed** or **Removed**, written for the person using the app: what they will see, not which function moved. Those notes become the GitHub Release and what the app shows as **Release notes** when it offers the update, and a release refuses to run while the section is empty.

They also decide the version number, so nobody has to:

| Unreleased lists | The release is | From 0.11.1 |
| --- | --- | --- |
| Something **Added** or **Removed** | A minor release: something a person would notice as new, or gone | 0.12.0 |
| Only **Changed** and **Fixed** | A patch | 0.11.2 |

That is the whole rule, and it lives in `nextVersion` in `scripts/changelog.mjs`, tested by `npm run scripts:test`. It follows that the heading a change is filed under matters: a new ability belongs under Added even when it is small, and a redesign of something that already exists belongs under Changed. You can always overrule it by giving a number yourself, which is how 1.0.0 will happen: leaving 0.x behind is a judgement about stability that no heading can make. Whatever the number, it must be higher than the last, because running apps only ever install a higher version.

### The button

On GitHub, open **Actions**, pick **Release** on the left, press **Run workflow**, and press the green button. Leave the version blank and the changelog decides it; type one to choose yourself. The run's page says which number it took and why. Only people with write access to the repository can do this. The workflow then:

1. Works out the version if none was given, and refuses if the version is not higher than the last release, if CI has not passed on the latest commit to main, or if the changelog has nothing under Unreleased.
2. Dates the Unreleased section as the version and opens a fresh one, sets the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json`, updates `Cargo.lock`, commits and tags.
3. Creates the GitHub Release as a draft, with the changelog section as the notes.
4. Builds a Windows installer (NSIS, per-user, no admin prompt) and a universal macOS disk image, and uploads them to the draft.
5. Publishes the release once both installers and the updater manifest are on it, so "latest" never points at a release with nothing to download.

The draft is made first for a second reason: the workflow's token can only create a release for a commit that matches the head of main in its workflow files, so it has to happen while the tag is fresh. Pushing to main while a release is building is fine.

Tick **Rehearse** to see steps 1 and 2 in the log without anything being pushed, built or published, which is also the way to ask what number the next release would be. A rehearsal can run from any branch.

`install.ps1` and `install.sh` at the repo root read `releases/latest`, so the moment the workflow finishes the one-line install picks up the new version, and running apps see it within the hour.

### From the terminal

The same thing from a checkout, for when you would rather:

```bash
npm run release
```

```bash
git push --follow-tags
```

The first command fetches the tags, works out the number the same way and says which it chose; `npm run release -- 0.12.0` chooses for it. It is exactly what the button runs, so the two routes cannot drift. The tag push starts the Release workflow at the build step. The terminal route does not check CI for you.

## Rolling back a release

If a release turns out to be broken, open **Actions**, pick **Roll back**, press **Run workflow**, give the last good version and one sentence on why. The workflow then:

1. Marks every release newer than the good one as a pre-release, which withdraws it. GitHub's "latest" points at the good version again at once, so the one-line installers hand out the good version and apps that have not updated yet are no longer offered the bad one.
2. Puts the good version's code back on main as one commit, **Roll back to vX**, with a line in the changelog saying so and why. The release tooling (`.github`, `scripts`, `CHANGELOG.md`) keeps its current state.
3. Releases that code as the next patch number, higher than the withdrawn release. Apps that already took the bad version update to the good code within the hour.

Step 3 is why a rollback is a release and not a deletion. The updater compares version numbers and only ever installs a higher one, so the only way to move an app off 0.9.0 is to give it a 0.9.1. Anyone who wants an older build by hand can still download it from the releases page; the withdrawn ones stay there, marked pre-release.

Work on main that landed after the good version is no longer on main afterwards. It is not lost: bring it back with `git revert` of the Roll back commit, fix what was wrong, and release again.

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
