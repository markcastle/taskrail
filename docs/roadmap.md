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
- Auto-update: a signed manifest with every release, a check on start and every hour, an "Install now" banner on the rail, and an **Update automatically** switch in Settings.
- Blueprints: a registry of ready-made profiles in the repository, matched by a server's tools before any model is asked, with `{me}` and `identity` so one profile serves everyone, and a Blueprint button on each connector to share yours.
- Blueprint submissions: Submit for review opens a prefilled form, a workflow checks the blueprint and turns it into a pull request, a maintainer merges, and running apps pick it up within the hour.
- Pause a connector: its tasks stay on the rail marked paused, nothing is pulled or written back, a task from it refuses to change status until it is resumed, and resuming pulls straight away.
- A trail on every task from a connector, board › group › parent, shown under the title and in the details, resolved by lookups a profile declares.
- Choices: a blueprint declares switches and picks, each person answers them under Filter on their own connector, so "only mine" can be turned off for a private board and one group on one board is two ticks.
- Calendars: any iCalendar feed, which covers Google, Outlook and Apple, added by its address with no MCP server involved. Repeating events expanded, today's events in a Today section, the next one as Up next. The address lives in the credential store.
- Auto-hide: the rail tucks into an 8px strip at the edge that other windows still keep clear of, comes out when the pointer is pushed against that edge or the shortcut asks for it, and goes back when the pointer leaves, unless someone is typing in it or dragging. The shell watches the pointer itself and the rules are a tested state machine in the core. A pin at the top of the rail is the same setting, one click away. Drag the inner edge to resize, with the width saved on release. An About page.
- Animation and sound, both optional. A ticked task slides away and the gap closes, a new task arrives, Up next fades in, the progress bar fills. Animation is on by default and off with one switch or the system's reduce-motion setting. Sound is a quiet tick on complete and a softer one on start, off until turned on, with the same switch in Settings and as **Silence Task Rail** in the tray.

## 🔜 Next

### More on a task

Tapping a task should show everything worth knowing: the description, who else is on it, the labels, a link straight to it in the source system, when it last changed. The profile learns to carry those fields, discovery learns to find them, and the details panel learns to lay them out without becoming a form. Where a task sits is done: see the trail, below.

### Discovery without an API key

Pasting an API key into an app you just installed is the biggest piece of friction in Task Rail, and it is friction at exactly the wrong moment: before the app has earned any trust. The aim is for most people never to see the Model page. Three layers, in the order they will land:

1. **Ready-made profiles.** Done, as blueprints: see above. What remains is filling the registry. Jira, Linear, Trello, Monday and GitHub Issues each need one person to connect them once and press Blueprint.
2. **Discovery by plain rules.** Many servers are obvious: a read-only tool called `list_issues` or `get_tasks` returning objects with `id` and `title`. A rule-based discoverer tries those first and only gives up when the server is genuinely unusual. No model, no key.
3. **Bring your own agent.** People who want Task Rail usually already have an agent they trust with a key: Claude Desktop, Claude Code, Cursor. Task Rail exposes a small MCP server of its own, and that agent does the discovery through it: list the server's tools, explore, propose a profile, test it. The key never enters Task Rail. This is the clearest answer so far to what the rail's MCP server is for.

The API key stays as the last resort, clearly labelled as such, for a server none of the three can handle.

### Signed builds

A Windows code-signing certificate and an Apple Developer ID, stored as repository secrets and used by the Release workflow. Until then the one-line install clears the first-run warnings, but anyone who downloads an installer by hand from the releases page meets SmartScreen or Gatekeeper.

## 🧭 On the list

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
