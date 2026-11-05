# Plan

The full plan, with the reasoning behind every decision and the drawings, is a living document maintained outside the repo. This page is the short version so the repo stands on its own.

## What it is

A permanent task strip docked to the edge of the desktop. The next few things you should do are always visible. Dragging a card between sections changes the task's status in the system it came from. Tasks come from any MCP server; calendars blend in as timed items.

## Decisions

| Topic | Decision |
| --- | --- |
| Stack | Rust workspace, Tauri 2 shell, React frontend |
| Source of truth | Moving a card writes the status back to the remote system |
| First connector | A generic MCP connector, tested against an internal tool. Jira and friends later, through the same contract |
| Calendars | A source like any other. Events carry a time and no status |
| LLM | Bring your own key: Anthropic, OpenAI or OpenRouter. Used at setup only |
| macOS | Parity from day one: floating panel, plus opt-in Accessibility window-nudging |
| Sections | Up next, Priorities, In progress, Later (the fold), Completed. User-defined later |
| The fold | Five shown by default, ten at most, the last card fades when more exist |
| Active task | One Active card among many In Progress. Local only, no timer |
| Rail as MCP server | In v1, so agents can read and push tasks |
| Discovery safety | Read-only tools run freely, writes prompt |
| Default edge | Right, with left available in settings |
| Licence | MIT |

## Milestones

1. **Docking spike.** The strip behaves on both platforms in a near-empty window, with CI building both.
2. **Core and local tasks.** SQLite store, three sections, drag and flick, active task mode, quick-add hotkey.
3. **Generic MCP connector.** MCP client, LLM trait, discovery wizard, saved profiles, deterministic two-way sync. Calendar events arrive here as a second profile.
4. **Agents and release.** The rail as an MCP server, tray, settings, updater, first public build.

## Open questions

- Which sign-in does the first target server use: the MCP authorization flow, a static token, or none?
- Google Calendar through an MCP server, or a first-party connector on the Calendar API?
- Is one hour the right window for pulling an upcoming meeting into Priorities?
- Which global shortcuts for quick-add and for toggling the rail?
