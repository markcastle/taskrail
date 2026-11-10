// The changelog, read and written by the release tooling.
//
//   node scripts/changelog.mjs section 0.8.0     print that version's notes
//   node scripts/changelog.mjs release 0.9.0     turn "Unreleased" into 0.9.0, dated today
//
// CHANGELOG.md follows Keep a Changelog: "## [Unreleased]" at the top collects
// changes as they land, and cutting a release gives that section the version
// and the date and starts a fresh Unreleased above it. The Release workflow
// prints the version's section into the GitHub Release, which is what the app
// opens from "Release notes".

import { readFileSync, writeFileSync } from 'node:fs'

const PATH = 'CHANGELOG.md'
const EMPTY = 'Nothing yet.'
const REPO = 'https://github.com/markcastle/taskrail'

const heading = (version) => new RegExp(`^## \\[${version.replace(/\./g, '\\.')}\\]`, 'm')

// The text between a version's heading and the next one, with the blank
// lines around it trimmed. Empty when the version is not in the file.
export function section(text, version) {
  const start = text.search(heading(version))
  if (start < 0) return ''
  const afterHeading = text.indexOf('\n', start) + 1
  const rest = text.slice(afterHeading)
  const next = rest.search(/^## \[/m)
  const body = (next < 0 ? rest : rest.slice(0, next)).replace(/^\[[^\]]+\]: .*$/gm, '')
  return body.trim()
}

// Retitles Unreleased as the version, adds the compare link, and opens a new
// Unreleased section. Refuses when there is nothing to release, because a
// release with no notes is a release nobody can read about.
export function release(text, version, date) {
  const notes = section(text, 'Unreleased')
  if (!notes || notes === EMPTY) {
    throw new Error(`CHANGELOG.md has nothing under "## [Unreleased]". Say what changed in ${version} before releasing it.`)
  }
  if (heading(version).test(text)) throw new Error(`CHANGELOG.md already has a section for ${version}.`)
  const previous = text.match(/^## \[(\d+\.\d+\.\d+)\]/m)?.[1]
  let out = text.replace(/^## \[Unreleased\]$/m, `## [Unreleased]\n\n${EMPTY}\n\n## [${version}] - ${date}`)
  out = out.replace(/^\[Unreleased\]: .*$/m, [
    `[Unreleased]: ${REPO}/compare/v${version}...HEAD`,
    previous ? `[${version}]: ${REPO}/compare/v${previous}...v${version}` : `[${version}]: ${REPO}/releases/tag/v${version}`,
  ].join('\n'))
  return out
}

const [command, version] = process.argv.slice(2)
if (import.meta.url === `file://${process.argv[1]}`) {
  if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
    console.error('Usage: node scripts/changelog.mjs <section|release> <major.minor.patch>')
    process.exit(1)
  }
  const text = readFileSync(PATH, 'utf8')
  if (command === 'section') {
    process.stdout.write(section(text, version) + '\n')
  } else if (command === 'release') {
    const today = new Date().toISOString().slice(0, 10)
    try {
      writeFileSync(PATH, release(text, version, today))
    } catch (error) {
      console.error(error.message)
      process.exit(1)
    }
    console.log(`CHANGELOG.md: Unreleased is now ${version} (${today}).`)
  } else {
    console.error(`Unknown command "${command}". Use section or release.`)
    process.exit(1)
  }
}
