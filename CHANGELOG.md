# Changelog

Every change to Task Rail that a person would notice, by version, newest first. Each release has what was **added**, what was **changed**, what was **fixed** and what was **removed**, written for the people using the app rather than for the people building it. The Release workflow copies a version's section from here into its GitHub Release, which is what the app opens from **Release notes** when it offers an update.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with one plain rule: a patch is for fixes and small adjustments, a minor is for anything you would notice as new. How a section gets here is in [docs/developing.md](docs/developing.md#cutting-a-release).

## [Unreleased]

Nothing yet.

## [0.8.1] - 2026-11-10

### Fixed

- A blueprint that matched a server but found nothing for you was treated as a failure. Discovery fell through to the model and, with no API key saved, told you to set one up. Now it connects with an empty rail and says so, pointing at **Scope** where "only tasks assigned to me" can be turned off. This was hitting people whose tracker had nothing assigned to them yet.

## [0.8.0] - 2026-11-10

### Added

- **Calendars on the rail.** Any calendar that publishes an iCalendar feed, which covers Google, Outlook and Apple, can be added under ⚙️ → Connectors → **Add a calendar** by pasting the feed's address. No MCP server and no profile are involved. The next fourteen days of events are fetched, refreshed every five minutes, and today's remaining events sit in a **Today** section with their times. An event about to start still joins Priorities and becomes Up next.
- Repeating events are expanded properly: RRULE, RDATE and EXDATE are honoured, an occurrence the feed has moved appears once in its new place, cancelled events are left out, and all-day events, timed events and events given as a duration all work.
- A calendar is a connector like any other, so **Pause**, **Sync now** and **Remove** behave the same, and the footer counts it. Its address is kept in the credential store, since anyone who has it can read the calendar.

### Changed

- The **Animation** and **Sounds** switches moved to the bottom of the Rail page, under a **Feedback** heading, so the settings people change most often come first.
- Calendar cards say what they are and do not offer Scope, Blueprint or Raw sample, because there is no profile behind them.

## [0.7.0] - 2026-11-10

### Added

- **Animation, on by default.** Every gesture gets an answer. Ticking a task's circle slides it away and closes the gap, a new task arrives rather than appears, the Up next card fades in when it changes, the progress bar's new segment eases in, and the update banner and notices rise into place.
- **Sound, off by default.** A quiet rising tick when you complete a task and a softer note when you start one, generated on the spot so nothing is shipped or loaded.
- Both are switches in Settings. Animation also turns itself off when your system asks for reduced motion. Sound can be silenced from the tray with **Silence Task Rail**, which stays in step with the switch in Settings. With animation off, nothing on the rail moves on its own, transitions included.

## [0.6.0] - 2026-11-10

### Added

- **A trail on every task.** A task from a connector shows where it sits, as Board › Group › Parent, under its title and in full in the details. A subtask called "SOP" now says which board and parent it is under. The trail is declared by the profile, so it works for any system, not just one.
- **Scope.** A blueprint can declare switches and picks, and you answer them on your own connector under **Scope**, with toggles and tick lists filled from the live server. "Only tasks assigned to me" is a switch, on by default, which someone with a private board turns off. Boards and groups are picks, so one group on one board is two ticks. Answers change what is pulled at once.
- The demo tracker gained groups, a subtask and an archived ticket, so the demo blueprint shows the trail and Scope working.

### Changed

- The Tuesday blueprint uses the trail and Scope, built from a sample a user sent in.
- Discovery is told to write profiles with a trail and choices, so new connectors get them too.

### Fixed

- The browser test could fail just after midnight, because the stand-in's completed tasks were stamped two hours ago and fell into yesterday. They are always today's now.

## [0.5.0] - 2026-11-09

### Added

- **The Tuesday blueprint,** the first to come through the submission form. Tuesday connects with no model and no key: add the connector, sign in, done.
- **Raw sample** on each connector shows what the server really returned for the list call, trimmed, for anyone writing or fixing a profile.
- **Set up a model** appears on a discovery that needs a model and has no key, so you are taken to the right place rather than told to find it.

### Changed

- **The past-due headline is a button.** "One task past due" used to leave you to find it, usually behind the fold. Press it and the rail shows only what is past due, across every section, with **Show everything** to go back. It switches itself off once nothing is late. The footer says "Up to date" beside the sync time.
- **A connector is added the moment discovery succeeds,** since it has just worked against the live server. The result says "Tuesday connected. 47 tasks on the rail" with **Undo** beside it and **Done** to close. The save button that scrolled out of view is gone. The one case that still asks first is a tool the server does not promise is read-only.
- The connector card is two lines and a row of quiet actions, with the address shortened to its host, instead of a column of one word per line.
- **Add server** is now **Add connector**, and its card says known systems connect on their own.
- The Model page moved to the end of Settings and says it is only needed the first time anyone connects a new kind of system.
- The README asks Mac users for feedback without calling the build untested.

### Fixed

- Blueprint submissions were being skipped because the `blueprint` label did not exist in the repository. The label exists, and the workflow also runs for any issue whose title starts with "Blueprint:".
- An exported blueprint no longer writes `null` for fields it does not use, so it is as short as it can be.

## [0.4.0] - 2026-11-06

### Added

- **Pause a connector.** Each connector has **Pause** and **Resume**. A paused connector's tasks stay on the rail marked paused, nothing is pulled or written back, and a task from it refuses to change status with a note saying why, since the change would not reach the system and the next pull would undo it. Resuming pulls straight away. The footer says when every connector is paused.
- The website brief lives with the docs, for whoever builds taskrail.uk.

### Changed

- The install lines in the README point at taskrail.uk.
- A refused action on the rail is a notice at the top, not a whole-rail error screen.

### Fixed

- **The Mac installer failed on its last line.** It copied the app into Applications and then opened it by name, which does not always work for an app that has just been copied. It opens by path now, checks the disk image held an app and the copy landed, and says where the app is if opening still fails.

## [0.3.1] - 2026-11-06

### Changed

- **Update messages say one thing.** Check for updates answers with one line: up to date, or the version that is available with a **Release notes** link and **Install and restart**. The banner on the rail says the version is available and nothing more.

## [0.3.0] - 2026-11-06

### Added

- **The Tuesday blueprint** joins the registry, with the person's email replaced by `{me}` so one profile serves everyone.
- **A Blueprint box on Add connector.** A blueprint pasted there is used instead of the registry and the model, so one can be tried before it is merged or used from a colleague.
- Paste profile recognises a blueprint and says to use Add connector instead, rather than failing on a missing field.
- **Release notes you can read.** Every release carries a list of what changed, one line each. The app shows the first change under the update banner and the whole list in Settings.

### Changed

- **The server names the connector.** The name on the Add connector form is optional. Left blank, the connector is named by the blueprint that fitted, else by what the server calls itself, else by its host. If the name is taken the id gets a number: "tuesday-2".
- The footer says "Synced 4 min ago" and keeps counting, instead of a bare clock time that read as the time of day.
- Sign-in credentials are keyed by the server's host rather than the connector id. Connectors made before this still find theirs.

### Fixed

- The update banner and the settings page were showing the release page's install instructions, markdown and all, because that was all the release had to say.

## [0.2.0] - 2026-11-06

### Added

- **Submit a blueprint for review.** **Submit for review** on the Blueprint panel opens a GitHub issue form with the blueprint filled in. A workflow checks it, refuses anything carrying a server, a name, a secret or a `{me}` without an identity, runs the registry's tests against it and opens a pull request for a maintainer. Once merged, running apps pick it up within the hour.
- The release guide says how to pick a version number.

### Changed

- The app checks for updates every hour instead of every four, so a release does not sit unnoticed for most of a working day.
- The app can open pages only under the project's own GitHub address.

## [0.1.3] - 2026-11-06

### Added

- **Blueprints.** A blueprint is a connector profile with the personal parts left out, plus the tool names it needs. The registry lives in the repository and the app fetches it when a server is added, with a copy compiled in as a fallback. Discovery lists the server's tools and tries a fitting blueprint first: instantiate, test live, show for approval, no model involved. Only when none fits is a model asked.
- Profiles gain **identity**: the tool that says who the signed-in person is and where their id lives. `{me}` in a profile is filled in on every connection, so one profile serves everyone.
- Each connector has a **Blueprint** button that shows its profile with server and secrets removed, ready to share.
- The demo tracker knows who you are, and the demo blueprint connects it with no model.

### Changed

- With no API key saved and no blueprint fitting, discovery says exactly that rather than failing obscurely.

## [0.1.2] - 2026-11-06

### Fixed

- The **Updates** heading in Settings sat on top of the next row's border, which drew a line through it. A subhead inside a group is a divider now.

## [0.1.1] - 2026-11-06

### Added

- **The app keeps itself current.** Every release is signed and carries a manifest. The app checks it twenty seconds after start and every four hours. A newer release installs at once when **Update automatically** is on, otherwise the rail shows a quiet banner with **Install now** and **Not now**. Settings has **Check for updates** with the result inline.
- **A public roadmap** at docs/roadmap.md: what is done, what is next, what is on the list and what is unsettled.
- A README written for people, with badges, and the detail moved into docs.

### Changed

- The monitor setting is a row of tiles, each a little screen in the display's own aspect ratio with its name and resolution beneath, and the chosen one shows where the rail sits. Any number of displays wraps cleanly, where the old segmented control spilled out of its row with three.
- The README says plainly that the Mac build gets less time on a real machine, and why there is an API key.

### Fixed

- The reserved strip on Windows is given back before an update installs, because the installer ends the process without the normal exit path.

## [0.1.0] - 2026-11-06

The first release anyone could install.

### Added

- **The rail.** A docked strip on Windows that other windows lay themselves out around, and a floating edge panel on macOS, on the monitor you choose, at the edge and width you choose.
- **Gestures.** Drag to reorder, pull into In progress, flick sideways to complete, tap the circle to complete, tap a task for details. Direction, not a hold timer, decides between a drag and a flick.
- **Your day with a shape.** Today's date, a headline, how many tasks are done, Up next, and Priorities, In progress, Later and Completed, with Priorities and In progress folded.
- **Quick-add** from anywhere with Ctrl+Shift+Space.
- **A tray icon** with Show, Hide, Settings and Quit. Quitting from the tray gives the reserved strip back on Windows.
- **Settings inside the rail** that save themselves: edge, width, theme, monitor, the fold, connectors, model and logs. A second window next to a docked strip hangs on Windows, so there is no second window.
- **Light, dark, or follow the system.**
- **The generic MCP connector.** Point it at any MCP server. A model reads the server's tools once and writes a Connector Profile, the app proves it against the live server, and from then on no model is involved. Profiles can explore first, fan out over boards and narrow a list with `where` clauses.
- **Sign in through the browser** to remote MCP servers, with the token carried through discovery and sync. Large credentials are stored in pieces so they fit the Windows Credential Manager.
- **Two-way sync.** Moving a task on the rail changes its status in the system it came from.
- **A demo tracker** the app serves itself, so there is something to connect to on day one.
- **A log file** and a Logs page in Settings, recording every step of connecting, discovering, signing in and syncing.
- **The icon and the blue.** One blue, flat surfaces, and the icon at every size.
- **Releases built by GitHub Actions** and a one-line install on Windows and macOS.

[Unreleased]: https://github.com/markcastle/taskrail/compare/v0.8.1...HEAD
[0.8.1]: https://github.com/markcastle/taskrail/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/markcastle/taskrail/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/markcastle/taskrail/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/markcastle/taskrail/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/markcastle/taskrail/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/markcastle/taskrail/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/markcastle/taskrail/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/markcastle/taskrail/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/markcastle/taskrail/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/markcastle/taskrail/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/markcastle/taskrail/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/markcastle/taskrail/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/markcastle/taskrail/releases/tag/v0.1.0
