# Blueprints

A blueprint is a Task Rail connector profile for a system someone has already connected, with the personal parts left out. When you add a server, Task Rail lists the server's tools, and if a blueprint here needs only tools the server has, it is used straight away, tested against the live server, and shown for approval. No model, no API key.

`registry.json` is the whole registry. The app fetches it from this repository's `main` branch when a server is added, and falls back to the copy built into it.

## Contributing one

1. Connect the system in Task Rail the usual way, through discovery or a pasted profile, and make sure it works: tasks appear, moving one writes back.
2. In Settings → Connectors, press **Blueprint** on the connector. That shows the profile with your server and secrets removed and the matching tools filled in.
3. If the profile names you, replace your user id with `{me}` and add an `identity` block that says which tool tells the app who the signed-in person is. The demo tracker entry shows the shape.
4. Add the entry to `registry.json` and open a pull request, or paste it into an issue if that is easier.

A good `description` says what the system is and what the profile pulls: "your open issues across every project", "tasks assigned to you on boards you belong to".

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
