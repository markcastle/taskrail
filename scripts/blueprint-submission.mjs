// Turns a "Submit a blueprint" issue into a change to the registry.
//
//   node scripts/blueprint-submission.mjs --body-file issue.md --registry blueprints/registry.json
//
// Reads the issue form's fields out of the body GitHub wrote, checks the
// JSON is a blueprint, adds it to the registry or replaces the entry with
// the same id, writes the file, and prints one line of JSON about what it
// did. Anything wrong is a message on stderr and exit code 1, which the
// workflow posts back on the issue.

import { readFileSync, writeFileSync } from 'node:fs'

const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => (a.startsWith('--') ? [a.slice(2), all[i + 1]] : [])).filter((p) => p.length))
if (!args['body-file'] || !args.registry) {
  console.error('Usage: blueprint-submission.mjs --body-file <issue body> --registry <registry.json>')
  process.exit(2)
}

const body = readFileSync(args['body-file'], 'utf8')

/** The text under a "### Heading" in a rendered issue form, up to the next heading. */
function field(heading) {
  const pattern = new RegExp(`^### ${heading}\\s*\\n([\\s\\S]*?)(?=^### |(?![\\s\\S]))`, 'm')
  const match = body.match(pattern)
  return match ? match[1].trim() : ''
}

function fail(message) {
  console.error(message)
  process.exit(1)
}

const system = field('What system is this\\?')
const pulls = field('What does the profile pull\\?')
let json = field('Blueprint JSON')
if (!json) fail('The "Blueprint JSON" field is empty.')
// A render: json textarea arrives inside a code fence.
json = json.replace(/^```[a-z]*\s*\n?/i, '').replace(/\n?```\s*$/, '').trim()
if (json === '_No response_') fail('The "Blueprint JSON" field is empty.')

let blueprint
try {
  blueprint = JSON.parse(json)
} catch (e) {
  fail(`The blueprint is not valid JSON: ${e.message}`)
}
if (!blueprint || typeof blueprint !== 'object' || Array.isArray(blueprint)) fail('The blueprint must be a JSON object.')
if (typeof blueprint.id !== 'string' || !/^[a-z0-9-]+$/.test(blueprint.id)) fail('"id" must be lower-case letters, digits and dashes, such as "linear".')
if (!blueprint.match || !Array.isArray(blueprint.match.tools) || blueprint.match.tools.length === 0) fail('"match.tools" must list at least one tool the server has.')
if (!blueprint.profile || typeof blueprint.profile !== 'object') fail('"profile" is missing.')
if (!blueprint.profile.list) fail('"profile.list" is missing: which tool returns the items?')
for (const key of ['server', 'id', 'name']) {
  if (key in blueprint.profile) fail(`"profile.${key}" must not be in a blueprint; it is filled in per person.`)
}
const text = JSON.stringify(blueprint)
for (const word of ['bearer_secret', 'oauth_secret', 'Authorization', 'token', 'password']) {
  if (text.includes(word)) fail(`The blueprint mentions "${word}". Nothing secret belongs in a blueprint.`)
}
if (text.includes('{me}') && !blueprint.profile.identity) fail('The profile says {me} but has no "identity" block saying which tool tells the app who the person is.')

// The issue's own answers win over whatever the export put in.
const [name, link] = system.split(/\s*,\s*|\s+(?=https?:\/\/)/)
if (name) blueprint.title = name.trim()
if (link && /^https?:\/\//.test(link)) blueprint.homepage = link.trim()
if (pulls) blueprint.description = pulls

const registry = JSON.parse(readFileSync(args.registry, 'utf8'))
registry.blueprints ??= []
const index = registry.blueprints.findIndex((b) => b.id === blueprint.id)
const action = index >= 0 ? 'update' : 'add'
if (index >= 0) registry.blueprints[index] = blueprint
else registry.blueprints.push(blueprint)
registry.blueprints.sort((a, b) => a.id.localeCompare(b.id))
writeFileSync(args.registry, JSON.stringify(registry, null, 2) + '\n')

console.log(JSON.stringify({ id: blueprint.id, title: blueprint.title, action, tools: blueprint.match.tools }))
