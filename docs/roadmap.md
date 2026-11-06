# Roadmap

Where Task Rail is going, in the order we expect to get there. This is a public document: if something here matters to you, say so in an issue, and if you want to build one of them, even better.

One rule shapes every item that touches the screen: **ask what Apple would do.** Fewer controls, better defaults, feedback for every action, nothing that needs explaining.

## ✅ Done

- Docking on Windows (a reserved strip) and macOS (a floating edge panel), on any monitor.
- Local tasks with drag to reorder, pull into In progress, flick to complete, tap for details.
- Quick-add from anywhere with Ctrl+Shift+Space.
- Settings inside the rail: edge, width, theme, monitor, the fold, connectors, model, logs.
- A tray icon with Show, Hide, Settings and Quit.
- The generic MCP connector: model-assisted discovery, sign-in through the browser, two-way sync, fan-out over boards, `where` filters.
- Releases built by GitHub Actions and a one-line install on both platforms.

## 🔜 Next

### Auto-update

The app checks for a new release on start and every few hours. When one exists, the rail says so quietly and offers **Install now**. A toggle in Settings, **Update automatically**, installs new releases without asking, at the next quiet moment. The Release workflow signs an update manifest so the app only ever installs what we built.

### Pause a connector

Each connector gets a pause switch. A paused connector keeps its tasks on the rail but stops pulling and stops writing back, and its rows say so. For when a system is being migrated, when it is noisy, or when you want a quiet afternoon.

### More on a task

Tapping a task should show everything worth knowing: the description, who else is on it, the labels, a link straight to it in the source system, when it last changed. The profile learns to carry those fields, discovery learns to find them, and the details panel learns to lay them out without becoming a form.

### Slicker interaction, with animation and sound

Every gesture gets an answer. A completed task slides away and the ones below close the gap. A drag lifts the row and settles it. A new task arrives rather than appearing. Up next changes with a crossfade, and the day's progress bar fills a segment as you finish something.

Sound is part of this. A short, quiet tick on complete, a softer one on start, nothing on anything routine. Sound is off until you turn it on, there is one switch for it in Settings, and the tray menu has **Silence Task Rail** so it can be quietened in one click.

### Discovery without an API key

Pasting an API key into an app you just installed is the biggest piece of friction in Task Rail, and it is friction at exactly the wrong moment: before the app has earned any trust. The aim is for most people never to see the Model page. Three layers, in the order they will land:

1. **Ready-made profiles.** A connector profile for each common system ships with the app or is fetched from this repository. Adding Jira, Linear, Trello, Monday or GitHub Issues becomes "sign in", with no model involved. A profile is a JSON document, so anyone who has connected a system can contribute theirs.
2. **Discovery by plain rules.** Many servers are obvious: a read-only tool called `list_issues` or `get_tasks` returning objects with `id` and `title`. A rule-based discoverer tries those first and only gives up when the server is genuinely unusual. No model, no key.
3. **Bring your own agent.** People who want Task Rail usually already have an agent they trust with a key: Claude Desktop, Claude Code, Cursor. Task Rail exposes a small MCP server of its own, and that agent does the discovery through it: list the server's tools, explore, propose a profile, test it. The key never enters Task Rail. This is the clearest answer so far to what the rail's MCP server is for.

The API key stays as the last resort, clearly labelled as such, for a server none of the three can handle.

### Signed builds

A Windows code-signing certificate and an Apple Developer ID, stored as repository secrets and used by the Release workflow. Until then the one-line install clears the first-run warnings, but anyone who downloads an installer by hand from the releases page meets SmartScreen or Gatekeeper.

## 🧭 On the list

### Calendar events

Events from your calendar on the rail, timed, with the next one as Up next when it is close. A calendar is a connector whose items carry a time instead of a status, so most of the plumbing exists; the open question is whether Google Calendar comes through an MCP server or a first-party connector.

### Ready-made profiles

Connector profiles for the common systems, shipped with the app, so Jira, Linear, Trello, Monday and GitHub Issues connect without a model at all. A profile is a JSON document; the first contributions here will likely be from people connecting their own tools.

### The rail as an MCP server

The original plan had the rail expose an MCP server so agents can read and add tasks. The first real use for it is above: letting the agent you already trust run discovery, so no key enters Task Rail. Beyond that, the candidates:

- An agent in a coding session saying "add a task to follow this up", and it appears on the rail.
- An agent reading what you are focused on right now and shaping its answers around it.
- One endpoint that gives an agent all of your work across every system you have connected, without it needing a connector for each.

If one of those would change how you work, say so. That decides how far past discovery this goes.

## 💭 Questions we have not settled

- Should Completed show today's tasks only, or everything finished this week?
- Is one hour the right window for pulling a meeting into Priorities?
- Which global shortcut should toggle the rail itself?
- When a task is on the rail from two systems at once, which wins?
