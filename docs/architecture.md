# Architecture

This is the explanation a new contributor should read once, with a coffee, before changing anything that is not a typo. It covers why the code is shaped the way it is, where the dangerous parts are, and what we have deliberately not done yet.

## The one rule

Dependencies point inward, and `core` is the centre.

`core` knows what a task is, what a section is, and what the rail should show at ten past three on a Tuesday. It does not know what SQLite is, what a window is, or what Jira is. Those things implement *ports*, which are plain Rust traits declared in `core/src/ports.rs`, and the adapters that implement them live in their own crates.

This is not purity for its own sake. It is what lets every product rule be a five-line test with no setup, and what makes "add a connector" mean "add a crate" rather than "edit twelve files".

## The crates

### `crates/core`

- `domain.rs` is the vocabulary: `Item`, `ItemKind`, `Status`, `Priority`, `Origin`, `TimeWindow`. An item is a task or a calendar event. Tasks move between sections; events are pinned to time and only ever arrive and leave.
- `settings.rs` is what the user can change about the rail itself: the edge, the width, the fold limit, how far ahead an event gets pulled in.
- `ports.rs` is the seams. Repository, key-value store, clock, secret store today; the connector capability traits join them in milestone three.
- `usecases.rs` is where the reading side of the product lives. `build_rail_view` takes a pile of items and returns exactly what to draw. If you want to know what happens to a meeting that started ten minutes ago, there is a test with that name.
- `services.rs` is the writing side. `RailService` is the only thing that changes items: quick-add, start, complete, reopen, move. It owns the rank arithmetic, including the renumbering when a drop lands between two adjacent ranks. Connector write-back will hang off these same methods.

### `crates/platform`

The operating system's opinion about where our window may go.

Windows registers an AppBar, the same mechanism the taskbar uses, so the OS shrinks the work area and maximised windows stop at our edge. macOS has no public equivalent. There, the rail is a floating window on every Space, and keeping other windows out from underneath it is a separate, opt-in feature behind the Accessibility permission. The trait is `DesktopDock`; the two implementations live in `windows.rs` and `macos.rs`; the not-yet-written window herder has its trait in `herder.rs` so the shell can be built against it.

Things worth knowing before you touch this crate:

- macOS calls must run on the main thread. `MainThreadMarker` enforces it; the shell hops over with `run_on_main_thread`.
- The Windows subclass procedure reads a single global `PLACEMENT`. That is deliberate. There is one rail per process and a global is the honest shape.
- Neither implementation can be tested on Linux. CI runs the real thing on both platforms; locally, cross-target `cargo check` catches the type errors.

### `crates/store-sqlite`

One SQLite file, two tables. `items` keeps each item as JSON in a `data` column with `status` and `rank` copied out beside it for the queries, so adding a field to `Item` is a one-line change in the core and a no-op here. `kv` holds the settings and the active item id. SQLite is bundled, so there is nothing to install. Everything is synchronous and runs on Tauri's command threads.

### `src-tauri`

The shell. It opens the store, builds a `RailService` over it in `rail.rs`, turns UI calls into service calls through `commands.rs`, emits a `rail:changed` event after every write so the UI refreshes, registers the global quick-add shortcut in `shortcuts.rs`, and hands the native window handle to the platform crate in `dock.rs`. On first run `seed.rs` adds three cards that teach the gestures. `logs.rs` writes a log file next to the data and the UI tails it.

There is exactly one window. A second webview window created next to a docked AppBar hung on Windows every time, so settings open inside the rail instead: the page swaps to the settings component and asks the shell to widen the strip with the same docking call used at startup, then gives the width back on close. Anything that needs more than a strip of screen should follow that pattern rather than opening a window.

### `ui`

React, TypeScript, Vite. `backend.ts` is the one interface the components talk to, with two implementations: Tauri commands in the app, and an in-memory stand-in with the same rules in a plain browser. The types mirror `core/src/domain.rs` by hand, and when one changes the other must.

The gestures are built from one library and one hook, and direction tells them apart. dnd-kit picks a card up after eight pixels of vertical movement, but gives up if the pointer goes twelve pixels sideways first. `useSwipe.ts` takes over in exactly that case and handles the flick on the same element. The first version used a press-and-hold delay instead, and a hand that started moving a fraction early cancelled the drag, which felt random. `ui/tests/rail.test.cjs` drives all of it in headless Chromium against the browser backend, and `npm run ui:test` is how you run it.

### `crates/connector-api`, `crates/connector-mcp` and `crates/sync`

Three crates carry milestone three. `connector-api` is the contract: one `Connector` trait with capability flags, `RemoteItem` as the shape every system translates into, and `StatusMap` for the three sections. `connector-mcp` is the only implementation so far and the one that matters most, described below. `sync` is the merge: `apply_remote` takes a connector's list and updates the repository, with the remote system winning on the fields it owns and the rail winning on rank. The shell's `sync.rs` runs the loop and the write-backs; `connectors.rs` keeps the registry of profiles in the key-value store.

The fake tracker in `connector-mcp/src/fake_server.rs` is worth knowing about. It is a real MCP server with deliberately unfamiliar vocabulary, it backs the end-to-end test, and the app runs it in-process when launched with `--fake-mcp-server`, which is how the demo connector works without shipping a second binary.

## How the generic MCP connector works

This is the most important design in the project.

A connector is any MCP server. When the user adds one, Task Rail fetches the server's tool list and hands it to a language model along with the capability contract. The model's job is to write a **Connector Profile**: a saved JSON document that says which tool lists the user's items and with what arguments, how to pull id, title, status, priority, URL and times out of the result, which tool changes status, and how the server's status values map onto To Do, In Progress and Done.

The profile is validated against the live server during setup, by calling the list tool for real and checking the extractors against a sample response. Read-only tools run freely. Anything that writes asks the user first.

After setup the model is out of the loop. Sync is deterministic: poll, apply the extractors, write status changes back through the mapped tool. The model is only called again if the server's tool list changes or a profile stops validating. Profiles are visible, editable and exportable, and good ones for popular servers ship with the app as presets.

A calendar is just a connector whose items carry a time window and no status.

### Discovery, as built

`connector-mcp/src/discovery.rs` runs up to eight model calls. Each answer is either an *exploration*, a request to call a few read-only tools so the model can see real data (which boards exist, what an id looks like, which statuses are in use), or a profile. A profile is parsed, has the parts the model must not decide overwritten (transport, id, name, kind), is validated, and is then run through the same `fetch_items` pipeline the connector uses. A failure becomes the next round's prompt with a trimmed sample of what the server really returned. A tool that is not read-only, by annotation or by a reading-verb name such as `list_` or `get_`, is refused during exploration and stops discovery with `NeedsConfirmation` if the profile depends on it, so the UI can ask the person first.

Exploration exists because of the first real server. Tuesday keeps work on boards and has no "list my tasks" tool: the model needed `list_boards` before it could write a profile, and the profile needed `for_each` to fetch every board's tasks. Both are in the format now. The tests in `tests/discovery.rs` drive all of this with a scripted model against the fake tracker, which has boards for the purpose.

The second lesson from Tuesday was volume. Fanning out over every board returned every task in the company, hundreds of them, most finished and most other people's. Many tools cannot narrow a list themselves, so the profile can: a `where` list on the list call keeps only items that pass every condition, and a `where` list on `for_each` keeps only the containers that do. A condition is a JSONPath on one element plus `equals`, `contains` (any case) or nothing (the path has a value), with `not` to invert it. "Only my tasks on the Platform boards" becomes an assignee check on each task and a name check on each board. The model is told to find the person's id with a whoami-style tool and the assignee field from a real item rather than guess, and the filter runs in `extract.rs` for both discovery and sync, so what the person sees during setup is what the rail will show.

`crates/llm` is the only place a model is called. It has one method, text in and text out, and two wire formats: Anthropic's Messages API spoken natively, and the chat completions shape for OpenAI and OpenRouter. The tests use a mock HTTP server to pin the request shape and the error typing. Keys live in the OS credential store through `src-tauri/src/secrets.rs`; the SQLite file only ever holds the provider and model name.

### Signing in

`connector-mcp/src/auth.rs` wraps the SDK's OAuth state machine. A sign-in binds a loopback port, asks the SDK to discover the server's authorization metadata and register a client (dynamic registration, or a pre-registered id later), opens the authorization URL in the browser via the shell, waits for the redirect on the loopback port, exchanges the code, and lets the SDK write the credentials to a `CredentialStore`. Our store serializes them as JSON into the OS credential store under `connector:<id>:oauth`; the profile records only that name. On every later connection the manager is rebuilt from the store and the SDK refreshes tokens as needed. A server that answers the first request with a 401 surfaces as `ConnectorError::Auth`, which discovery turns into `SignInRequired` so the UI can offer the button.

Still to come: presets for popular servers so most people never trigger the model at all, and pre-registered client ids for servers that refuse dynamic registration.

## What we have deliberately not done

- Multiple monitors. One rail on the primary monitor for now.
- Notes and attachments on a card. Local-only first, then synced, both later.
- User-defined sections. Three fixed ones until the status mapping is proven.
- Writing rail order back to the remote system. Order is local; remote priority is a badge.
- Code signing. Unsigned builds until there is something worth signing.
