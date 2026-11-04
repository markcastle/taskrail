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

The shell. It opens the store, builds a `RailService` over it in `rail.rs`, turns UI calls into service calls through `commands.rs`, emits a `rail:changed` event after every write so the UI refreshes, registers the global quick-add shortcut in `shortcuts.rs`, and hands the native window handle to the platform crate in `dock.rs`. On first run `seed.rs` adds three cards that teach the gestures.

### `ui`

React, TypeScript, Vite. `backend.ts` is the one interface the components talk to, with two implementations: Tauri commands in the app, and an in-memory stand-in with the same rules in a plain browser. The types mirror `core/src/domain.rs` by hand, and when one changes the other must.

The gestures are built from two libraries and one hook. dnd-kit handles press-and-hold drag with a 150 ms delay, which is what makes a card feel picked up rather than nudged. `useSwipe.ts` handles the sideways flick on the same element and leaves vertical movement alone, so the two never fight. `ui/tests/rail.test.cjs` drives all of it in headless Chromium against the browser backend, and `npm run ui:test` is how you run it.

## What the generic MCP connector will look like

This is milestone three, and the most important design in the project, so it is written down before the code exists.

A connector is any MCP server. When the user adds one, Task Rail fetches the server's tool list and hands it to a language model along with the capability contract. The model's job is to write a **Connector Profile**: a saved JSON document that says which tool lists the user's items and with what arguments, how to pull id, title, status, priority, URL and times out of the result, which tool changes status, and how the server's status values map onto To Do, In Progress and Done.

The profile is validated against the live server during setup, by calling the list tool for real and checking the extractors against a sample response. Read-only tools run freely. Anything that writes asks the user first.

After setup the model is out of the loop. Sync is deterministic: poll, apply the extractors, write status changes back through the mapped tool. The model is only called again if the server's tool list changes or a profile stops validating. Profiles are visible, editable and exportable, and good ones for popular servers ship with the app as presets.

A calendar is just a connector whose items carry a time window and no status.

## What we have deliberately not done

- Multiple monitors. One rail on the primary monitor for now.
- Notes and attachments on a card. Local-only first, then synced, both later.
- User-defined sections. Three fixed ones until the status mapping is proven.
- Writing rail order back to the remote system. Order is local; remote priority is a badge.
- Code signing. Unsigned builds until there is something worth signing.
