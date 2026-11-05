# Task Rail

A permanent task strip docked to the edge of your desktop, fed by whatever systems your work already lives in.

Task Rail is not another to-do list you have to remember to open. It claims a narrow column at the side of the screen, Windows lays everything else out around it, and the next few things you should do are always in front of you. Drag a card to reprioritise, pull it into In Progress to start it, flick it away when it is done, and the change is written back to the system the task came from.

It is open source under the MIT licence and very early. Expect rough edges.

## Where it gets its tasks

That is the interesting part. Task Rail speaks [MCP](https://modelcontextprotocol.io) rather than any one vendor's API. Point it at an MCP server, and it works out which tools list your work, which one changes a status, and how that system's statuses map onto the rail's three sections. The first server it targets is an internal tool; Jira, Trello, Monday, Linear and calendars arrive through the same door.

The plan, with every decision and why, lives in the project doc linked from the first issue. A shorter version is in [docs/plan.md](docs/plan.md).

## Status

| Milestone | State |
| --- | --- |
| 1. Docking spike: a reserved strip on Windows, a floating edge panel on macOS | Built, awaiting hands-on testing on both platforms |
| 2. Core and local tasks: sections, cards, active task mode, quick-add | Built: SQLite store, drag to reorder, flick to complete, quick-add with a global shortcut, settings |
| 3. Generic MCP connector: discovery, profiles, two-way sync | Built: profiles, the MCP client, two-way sync, a demo tracker, model-assisted discovery, and the MCP sign-in flow for remote servers. Awaiting a hands-on run against a real remote server |
| 4. Rail as an MCP server, tray, settings, release pipeline | Not started |

## Running it

You need a stable Rust toolchain, Node 22 or later, and the usual [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
```

```bash
npm run dev
```

That starts the Vite dev server and launches the rail with hot reload. On Linux the rail cannot reserve screen space, so it falls back to an always-on-top window at the edge, which is enough to work on the UI.

To hack on the frontend alone in a browser, with sample data and no Rust involved:

```bash
npm run ui:dev
```

To run the Rust tests, which is where the product rules are specified:

```bash
cargo test --workspace
```

To drive the UI through its gestures in a headless browser, the way CI does:

```bash
npx playwright install chromium
```

```bash
npm run ui:test
```

## Using it

- **Drag** a card up or down to reorder it, or drop it in another section.
- **Flick** a card sideways to complete it.
- **Tap** a card to open it. From there you can start it, make it the active one, send it back, or delete it.
- **Ctrl+Shift+Space** anywhere on the desktop opens quick-add. Enter saves, Escape closes.
- **The gear** opens settings inside the rail, which widens to make room and narrows again when you go back. Appearance, edge, width, the fold, the event window, the model, connectors and the log all live there, along with Quit.
- **The tray icon** brings the rail forward and has Quit too. Always quit through one of those rather than killing the process: on Windows the reserved strip is only given back when the app exits cleanly. If the desktop is ever left shrunk, start the rail again and quit it properly, or restart Windows Explorer from Task Manager.

Your data lives in `rail.sqlite` in the app's data directory for your user. Delete that file for a fresh start.

## Connecting a system

Open the gear and look at the Connectors box.

- **Add demo tracker** connects a small fake issue tracker that ships inside the app. Its tickets appear as cards within a few seconds, marked with their source. Drag one into In Progress and the tracker's ticket moves state too. It's the quickest way to see two-way sync working.
- **Add server** is the real thing. Give it a name and either a URL or a command, press Discover, and a model reads the server's tool list, writes a Connector Profile, and the app tests that profile against the live server before showing you what it found. You see the first few items it pulled and the profile itself, which you can edit, then save. If the model picks a list tool the server doesn't promise is read-only, discovery stops and asks before calling it. If the server wants you to sign in first, discovery stops with a Sign in button: your browser opens, you sign in there, and when it comes back discovery carries on by itself. The resulting credentials live in your credential store and are refreshed automatically. Discovery needs a model: set one up under **Model** in the same panel with your own key for Anthropic, OpenRouter or OpenAI. The key goes in your operating system's credential store, never in a file.
- **Paste profile** takes a Connector Profile directly, a JSON document that says how to reach an MCP server, which tool lists your items, how to read each field out of the result, which tool changes a status, and how its status words map onto the rail's sections. A profile can also say "for each board, fetch its tasks" for systems that organise work in containers. The profile for the demo tracker in `crates/connector-mcp/src/profile.rs` is the worked example.

The model is only involved at setup. Once a profile exists, every pull and every write-back is plain code with no model in the loop.

Cards from a connector show its id as a small tag. Their title, status, priority, due date and link belong to the remote system and are refreshed on every pull. Their position in the rail belongs to you and is never touched by a pull.

## How the code is laid out

| Path | What lives there |
| --- | --- |
| `crates/core` | The domain model, the ports, and the rules for what the rail shows. No I/O, fully tested. Start here. |
| `crates/platform` | Desktop docking. The Windows AppBar and the macOS floating panel behind one trait. |
| `crates/store-sqlite` | Where items and settings live between runs. One SQLite file, bundled, nothing to install. |
| `crates/connector-api` | The contract every connector implements, and the small remote item model they all translate into. |
| `crates/connector-mcp` | The generic connector: any MCP server described by a Connector Profile. Also the fake tracker used for tests and demos. |
| `crates/sync` | Merging what a connector returns into the rail without disturbing what the user arranged. |
| `crates/llm` | One small door to a language model, used only by discovery. Anthropic natively, OpenAI and OpenRouter through the chat completions shape. |
| `src-tauri` | The Tauri shell: the window, the commands the UI calls, and the glue to the dock. |
| `ui` | The React frontend. Cards, sections, and soon the drag physics. |
| `docs` | Longer-form notes for people working on the code. |

Dependencies point inward. `src-tauri` depends on `platform`, `store-sqlite` and `core`; those depend only on `core`; `core` depends on nothing of ours. If a change in one crate forces a change in `core`, stop and ask whether it belongs there.

Read [docs/architecture.md](docs/architecture.md) before adding a connector or touching docking.

## Contributing

Issues and pull requests are welcome. Keep pull requests small, add a test in `crates/core` for any behaviour change, and run `cargo fmt` and `cargo clippy --workspace --all-targets` before pushing. CI runs on Windows, macOS and Linux.
