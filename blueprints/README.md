# Blueprints

A blueprint is a Task Rail connector profile for a system someone has already connected, with the personal parts left out. When you add a server, Task Rail lists the server's tools, and if a blueprint here needs only tools the server has, it is used straight away, tested against the live server, and shown for approval. No model, no API key.

`registry.json` is the whole registry. The app fetches it from this repository's `main` branch when a server is added, and falls back to the copy built into it.

## Submitting one

1. Connect the system in Task Rail the usual way, through discovery or a pasted profile, and make sure it works: tasks appear, moving one writes back.
2. In Settings → Connectors, press **Blueprint** on the connector. That shows the profile with your server and secrets removed and the matching tools filled in.
3. If the profile names you, replace your user id with `{me}` and add an `identity` block that says which tool tells the app who the signed-in person is. The demo tracker entry shows the shape.
4. Press **Submit for review**. Your browser opens the [submission form](https://github.com/markcastle/taskrail/issues/new?template=blueprint.yml) with the blueprint filled in. Say what the system is and what the profile pulls, and send it.

From there it is automatic: a workflow checks the blueprint with the same code the app uses, turns the issue into a pull request, and comments on your issue with the link. A maintainer reviews it, and once it is merged every running Task Rail picks it up within the hour. If the check finds a problem, the comment says what, and editing the issue runs it again.

You can also edit `registry.json` directly and open a pull request; the checks are the same.

A good `description` says what the system is and what the profile pulls: "your open issues across every project", "tasks assigned to you on boards you belong to".

## Reviewing one

The pull request the workflow opens has a short checklist. The automated part has already confirmed the JSON parses, instantiates as a profile, carries no server or secrets, and says `{me}` only with an `identity`. What needs a person:

- The description reads well and says what the profile pulls.
- The tool names belong to that system. A blueprint for one system that happens to match another's tools would connect the wrong profile.
- Nothing is specific to the submitter's workspace: a board id in `create`, a project key in a filter.
- If an entry with the same id exists, the update is an improvement, not a different system under the same name.

Merge, and it is live.

## The shape

```json
{
  "id": "demo-tracker",
  "title": "Demo tracker",
  "description": "What it is and what it pulls.",
  "homepage": "https://example.com",
  "match": {
    "tools": ["list_tickets", "transition_ticket", "whoami"],
    "url_contains": ["tracker.example.com"]
  },
  "profile": { "...": "a profile without id, name or server" }
}
```

`match.tools` is the fingerprint: every tool listed must exist on the server. Use the tools the profile actually calls, which is what the Blueprint button fills in. `url_contains` is optional and only breaks ties when two blueprints fit the same server.

The profile format itself, including `for_each`, `where` and `identity`, is described in [docs/connectors.md](../docs/connectors.md).
