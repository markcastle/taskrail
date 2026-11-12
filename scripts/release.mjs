// Cut a release: date the changelog, set the version everywhere it lives,
// commit, and tag.
//
//   npm run release              the changelog decides the number
//   npm run release -- 0.1.1     or you do
//   git push --follow-tags
//
// Left to itself the number comes from what CHANGELOG.md lists under
// "## [Unreleased]": something Added or Removed makes a minor release,
// anything else a patch. See nextVersion in changelog.mjs.
//
// It stops if CHANGELOG.md has nothing under "## [Unreleased]": the notes
// there become the GitHub Release and what the app shows as Release notes.
//
// The tag push starts .github/workflows/release.yml, which builds the
// installers and publishes the GitHub Release the install scripts read.

import { execSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

import { nextVersion } from './changelog.mjs'

const run = (cmd) => execSync(cmd, { stdio: 'inherit' })
const sh = (cmd) => execSync(cmd, { encoding: 'utf8' }).trim()

// The highest version tagged so far, as numbers and not as text: 0.10.0 is
// after 0.9.0.
function lastRelease() {
  const versions = sh("git tag -l 'v*'")
    .split('\n')
    .map((tag) => tag.replace(/^v/, ''))
    .filter((v) => /^\d+\.\d+\.\d+$/.test(v))
    .map((v) => v.split('.').map(Number))
    .sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2])
  return (versions.at(-1) ?? [0, 0, 0]).join('.')
}

let version = process.argv[2]
if (version === undefined) {
  // Releases cut from the button are tagged on GitHub, not here. Without
  // them this would count up from the last release this checkout knows of.
  try {
    execSync('git fetch --tags --quiet', { stdio: 'ignore' })
  } catch {
    console.warn('Could not fetch tags; working from the ones already here.')
  }
  try {
    const next = nextVersion(readFileSync('CHANGELOG.md', 'utf8'), lastRelease())
    version = next.version
    console.log(`Releasing ${version}: ${next.why}.`)
  } catch (error) {
    console.error(error.message)
    process.exit(1)
  }
}
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error('Usage: npm run release [-- <major.minor.patch>]')
  process.exit(1)
}

if (sh('git status --porcelain')) {
  console.error('Commit or stash your changes first; a release commit should carry only the version.')
  process.exit(1)
}

// First, so a changelog with nothing to say stops the release before any
// version number has moved.
run(`node scripts/changelog.mjs release ${version}`)

function rewrite(path, replacer) {
  const before = readFileSync(path, 'utf8')
  const after = replacer(before)
  if (after === before) throw new Error(`nothing changed in ${path}; is the version already ${version}?`)
  writeFileSync(path, after)
}

rewrite('package.json', (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`))
rewrite('src-tauri/tauri.conf.json', (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`))
rewrite('Cargo.toml', (s) => s.replace(/^version = "[^"]+"/m, `version = "${version}"`))
// Cargo.lock records the workspace crates' versions too.
run('cargo update --workspace --quiet')

run(`git commit -am "Release v${version}" --quiet`)
run(`git tag -a v${version} -m "Task Rail v${version}"`)
console.log(`\nTagged v${version}. Now push it to build the installers:\n\n  git push --follow-tags\n`)
