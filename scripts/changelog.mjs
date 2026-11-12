// The changelog, read and written by the release tooling.
//
//   node scripts/changelog.mjs section 0.8.0     print that version's notes
//   node scripts/changelog.mjs release 0.9.0     turn "Unreleased" into 0.9.0, dated today
//   node scripts/changelog.mjs note "text"       add a line under Unreleased's Changed
//   node scripts/changelog.mjs next 0.11.1       print what the next release should be numbered
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

// What the next release should be numbered, going by what Unreleased holds.
//
// The rule is the one in docs/developing.md, made mechanical so nobody has to
// remember it: something Added (or Removed) is something a person would
// notice as new, which is a minor release; only Changed and Fixed is a patch.
// The changelog is written as the work lands, so by release time it already
// knows which it is. Throws when there is nothing to release.
export function nextVersion(text, current) {
  const notes = section(text, 'Unreleased')
  if (!notes || notes === EMPTY) {
    throw new Error('CHANGELOG.md has nothing under "## [Unreleased]", so there is nothing to release. Say what changed first.')
  }
  const [major, minor, patch] = current.split('.').map(Number)
  const kinds = [...notes.matchAll(/^### (\w+)\s*$/gm)].map((m) => m[1])
  const noticeable = kinds.filter((kind) => kind === 'Added' || kind === 'Removed')
  if (noticeable.length > 0) {
    return { version: `${major}.${minor + 1}.0`, why: `a minor release, because the changelog lists something ${noticeable.join(' and ')}` }
  }
  return { version: `${major}.${minor}.${patch + 1}`, why: 'a patch, because the changelog lists only changes and fixes' }
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

// Adds one line under "### Changed" in Unreleased, making the heading if it
// is missing and dropping the "Nothing yet." placeholder. The roll back
// workflow uses it to record what it did.
export function note(text, line) {
  const start = text.search(heading('Unreleased'))
  if (start < 0) throw new Error('CHANGELOG.md has no "## [Unreleased]" section.')
  const afterHeading = text.indexOf('\n', start) + 1
  const rest = text.slice(afterHeading)
  const next = rest.search(/^## \[/m)
  let body = (next < 0 ? rest : rest.slice(0, next)).replace(`${EMPTY}\n`, '')
  if (/^### Changed$/m.test(body)) {
    body = body.replace(/^### Changed\n\n/m, `### Changed\n\n- ${line}\n`)
  } else {
    body = `\n### Changed\n\n- ${line}\n${body.trimEnd()}\n\n`.replace(/\n{3,}/g, '\n\n')
  }
  return text.slice(0, afterHeading) + body + (next < 0 ? '' : rest.slice(next))
}

const [command, argument] = process.argv.slice(2)
if (import.meta.url === `file://${process.argv[1]}`) {
  const version = argument
  const needsVersion = command === 'section' || command === 'release' || command === 'next'
  if (needsVersion && !/^\d+\.\d+\.\d+$/.test(version ?? '')) {
    console.error('Usage: node scripts/changelog.mjs <section|release|next> <major.minor.patch> | note "text"')
    process.exit(1)
  }
  const text = readFileSync(PATH, 'utf8')
  if (command === 'note') {
    if (!argument) {
      console.error('Usage: node scripts/changelog.mjs note "what changed"')
      process.exit(1)
    }
    writeFileSync(PATH, note(text, argument))
    console.log('CHANGELOG.md: noted under Unreleased.')
  } else if (command === 'next') {
    // The number alone on stdout, for scripts; the reason on stderr, for people.
    try {
      const next = nextVersion(text, version)
      console.error(`After ${version} comes ${next.version}: ${next.why}.`)
      process.stdout.write(next.version + '\n')
    } catch (error) {
      console.error(error.message)
      process.exit(1)
    }
  } else if (command === 'section') {
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
    console.error(`Unknown command "${command}". Use section, release, next or note.`)
    process.exit(1)
  }
}
