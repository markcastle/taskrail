// Cut a release: date the changelog, set the version everywhere it lives,
// commit, and tag.
//
//   npm run release -- 0.1.1
//   git push --follow-tags
//
// It stops if CHANGELOG.md has nothing under "## [Unreleased]": the notes
// there become the GitHub Release and what the app shows as Release notes.
//
// The tag push starts .github/workflows/release.yml, which builds the
// installers and publishes the GitHub Release the install scripts read.

import { execSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

const version = process.argv[2]
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('Usage: npm run release -- <major.minor.patch>')
  process.exit(1)
}

const run = (cmd) => execSync(cmd, { stdio: 'inherit' })
const sh = (cmd) => execSync(cmd, { encoding: 'utf8' }).trim()

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
