# Connecting Task Rail to your work

Task Rail reads tasks from any system that has an [MCP](https://modelcontextprotocol.io) server. This page is for when the two buttons in Settings are not enough: you want to know what discovery does, you want to steer it, or you want to write a connector by hand.

## Blueprints first

Before any model is involved, Task Rail checks whether the server is one somebody has connected before. It lists the server's tools and looks them up in the [blueprint registry](../blueprints/registry.json): a profile for each known system with the personal parts left out. If one fits, it is used straight away, tested against the live server, and shown for approval. No model, no API key.

The registry is fetched from this repository when you add a server, with a copy built into the app as the fallback. Contributing a blueprint is a **Blueprint** button on any saved connector and a pull request; [blueprints/README.md](../blueprints/README.md) has the steps.

## The three ways in

Open ⚙️ and find **Connectors**.

### Add demo tracker

A small fake issue tracker ships inside the app. Adding it gives you seven tickets on two boards, assigned to two people, so there is something to drag around and a status change to watch happen. It is also what the test suite runs against. Remove it from the same box when you are done.

### Add connector

Give the server a URL (or a command for a server that runs locally over stdio) and optionally a hint. The name is optional too: left blank, the blueprint's title or what the server calls itself in the MCP handshake is used, and the id is kept clear of connectors you already have ("tuesday-2"). Press **Discover**.

What happens next:

1. Task Rail connects and lists the server's tools.
2. If the server answers with a sign-in challenge, discovery stops and offers a **Sign in** button. Your browser opens, you sign in, and when the server sends you back discovery carries on by itself. The tokens go into your operating system's credential store and are refreshed automatically.
3. A language model reads the tool list and your hint, and may ask to explore: call a few read-only tools to see real data, such as which boards exist or what an id looks like. Only tools the server marks read-only, or whose names clearly read (`list_`, `get_`, `search_`, `whoami`), are allowed. Everything else is refused.
4. The model writes a **Connector Profile**: which tool lists your items, how to read each field out of the result, which tool changes a status, and how the system's status words map onto To do, In progress and Done.
5. Task Rail runs that profile against the live server. If it fails, the model gets the error and a sample of what the server really returned, and tries again. Up to eight rounds.
6. It worked against the live server, so it is connected at once. You see the first few items it pulled, with **Undo** beside them if this was not what you meant, and the profile under a disclosure for the curious.

If the model picks a list tool the server does not promise is read-only, discovery stops and asks you first.

**The hint matters.** "Only tasks assigned to me on the Platform board" tells the model to narrow the list, either through the tool's own arguments or with a `where` clause on the result. Without a hint you may get every task in the company.

**It needs a model only when no blueprint fits.** Then set one up under **Model** in the same panel, with your own key for Anthropic, OpenRouter or OpenAI. The key lives in your credential store. The model is only ever called during discovery; after that every pull and every write-back is plain code.

Have a blueprint that is not in the registry yet, from a colleague or one you are about to submit? Paste it into the **Blueprint** box on the Add server form. It is used instead of the registry and the model, with the server address and sign-in from the form.

### Paste profile

If you already have a profile, from someone else or from an earlier discovery, paste it in. A bearer token, if the server needs one, goes in the field beside it and into your credential store, never into the saved profile.

## What a Connector Profile looks like

A profile is a JSON document. This is the one for the demo tracker, trimmed:

```json
{
  "version": 1,
  "id": "fake-tracker",
  "name": "Fake tracker",
  "server": { "transport": "stdio", "command": "taskrail", "args": ["--fake-mcp-server"] },
  "kind": "task",
  "list": {
    "tool": "list_tickets",
    "arguments": { "assignee": "me", "open_only": false },
    "items": "$.tickets[*]",
    "fields": {
      "external_id": "$.ticket_id",
      "title": "$.summary",
      "status": "$.state",
      "priority": "$.priority",
      "due": "$.due_date",
      "url": "$.link"
    }
  },
  "set_status": {
    "tool": "transition_ticket",
    "arguments": { "ticket_id": "{external_id}", "state": "{status}" }
  },
  "create": {
    "tool": "create_ticket",
    "arguments": { "summary": "{title}" }
  },
  "status_map": {
    "to_section": { "Backlog": "todo", "Selected": "todo", "In Progress": "in_progress", "Done": "done" },
    "canonical": { "todo": "Selected", "in_progress": "In Progress", "done": "Done" }
  },
  "priority_map": { "P1": "highest", "P2": "high", "P3": "normal", "P4": "low" },
  "poll_seconds": 60
}
```

Field by field:

| Field | What it says |
| --- | --- |
| `server` | How to reach the server. `http` with a `url`, or `stdio` with a `command`. An HTTP server may name a `bearer_secret` or `oauth_secret`: those are names of entries in your credential store, never the secrets themselves. |
| `kind` | `task` or `event`. Events carry a time window instead of a status. |
| `list` | The tool that returns your items, its arguments, and a JSONPath to the array of items in the result. |
| `list.fields` | JSONPaths evaluated against one item. `external_id` and `title` are required. `status`, `priority`, `due`, `url`, `updated_at` are optional. Events use `start` and `end`. |
| `list.where` | Keeps only items that pass every condition. See below. |
| `list.for_each` | For systems that keep work in containers: call `list_boards` once, then the list tool once per board. See below. |
| `identity` | Which tool says who the signed-in person is, and where their id sits in its result. Lets the profile say `{me}` instead of carrying a user id. See below. |
| `set_status` | The tool that changes a status. `{external_id}` and `{status}` are filled in at call time. Leave it out and the connector is read-only. |
| `create` | The tool that makes a new item from a `{title}`. Optional. |
| `status_map.to_section` | Every status word the server might return, mapped onto `todo`, `in_progress` or `done`. |
| `status_map.canonical` | The one word to send for each of the three when a task moves. |
| `priority_map` | The server's priority words onto `lowest`, `low`, `normal`, `high`, `highest`. Case does not matter. |
| `poll_seconds` | How often to refresh. |

### Fanning out over boards

Some systems have no "list my tasks" tool, only "list boards" and "get board". Then `for_each` runs the parent call once and the list call once per element:

```json
"list": {
  "tool": "get_board",
  "arguments": { "board_id": "{board_id}" },
  "items": "$.tasks[*]",
  "fields": { "external_id": "$.id", "title": "$.name", "status": "$.status" },
  "for_each": {
    "tool": "list_boards",
    "arguments": {},
    "items": "$.boards[*]",
    "id": "$.id",
    "placeholder": "{board_id}"
  }
}
```

### Narrowing with `where`

When the tool cannot narrow the list itself, the profile can. Each condition is a JSONPath on one element plus one test. All conditions must pass.

```json
"where": [
  { "path": "$.assignees[*].id", "equals": "u-1234" },
  { "path": "$.archived", "not": true }
]
```

| Test | Passes when |
| --- | --- |
| `equals` | Any value at the path matches, ignoring case for strings. Numbers and their text compare equal. |
| `contains` | Any value at the path is a string containing the text, any case. |
| neither | The path has a value. |
| `not: true` | Inverts the result. |

A `where` on `for_each` filters the containers the same way, so `{ "path": "$.name", "contains": "platform" }` fans out over the Platform boards only.

### Saying "me" without a user id

A profile that filters on your user id works for you alone. With `identity`, it works for anyone:

```json
"identity": { "tool": "whoami", "arguments": {}, "id": "$.id" },
"list": {
  "where": [ { "path": "$.assignees[*].id", "equals": "{me}" } ]
}
```

On every connection the tool is called once, the id is read from the result, and `{me}` is filled in wherever it appears: list and parent arguments, and the values conditions compare against. Discovery writes profiles this way when a whoami-style tool exists, which is what makes a discovered profile shareable as a blueprint.

## Raw sample

**Raw sample** on a connector shows what the server actually returned for the list call, trimmed: the parent call's result when there is a fan-out, then the list call for the first element. It is for working out which fields hold what when writing a profile or adding context fields to a blueprint.

## Pausing a connector

**Pause** on a connector keeps its tasks on the rail, marked paused, and stops pulling and writing back. A task from a paused connector refuses to change status, with a note saying why, because the change would not reach the system and the next pull after resuming would undo it. **Resume** pulls straight away. The rail's footer says when every connector is paused.

## What sync does and does not touch

Every `poll_seconds` the connector pulls the list and merges it:

- Title, status, priority, due date and link belong to the remote system and are refreshed every pull.
- The position of a task on the rail belongs to you. A pull never moves a task.
- A task that disappears from the remote list is removed from the rail.
- When you move a task on the rail, the canonical status word is sent through `set_status` before anything else happens. If the server refuses, the task snaps back and the error shows at the bottom of the rail.

Tasks from a connector show its name in grey under the title.

## When it goes wrong

Open ⚙️ → **Logs**. Discovery logs every round: what it asked, what the model answered, what the server returned. Paste the lines from "discovery for" onward into an issue and it is nearly always enough to see what happened.

The most common causes:

- **"No working profile after 8 rounds."** The model could not find a tool that lists your work. A hint naming the board or project usually fixes it.
- **Hundreds of items.** The profile fans out over every container. Add a hint about which one is yours, or edit the profile and add a `where`.
- **"Sign in required" every time.** The server's sign-in tokens expired and could not be refreshed. Remove the connector and add it again.
- **A task snaps back after a drag.** The server refused the status change; the error at the bottom of the rail says why. Often the canonical word in `status_map` is not one the server accepts.
