// node --test scripts/
//
// The release number is worked out from the changelog, so the rule that does
// it is tested like any other rule.

import assert from 'node:assert/strict'
import { test } from 'node:test'

import { nextVersion, release, section } from './changelog.mjs'

const log = (unreleased) => `# Changelog

## [Unreleased]

${unreleased}

## [0.11.1] - 2026-11-12

### Fixed

- Something.

[Unreleased]: https://github.com/markcastle/taskrail/compare/v0.11.1...HEAD
[0.11.1]: https://github.com/markcastle/taskrail/compare/v0.11.0...v0.11.1
`

test('only fixes and changes make a patch', () => {
  assert.equal(nextVersion(log('### Fixed\n\n- A bug.'), '0.11.1').version, '0.11.2')
  assert.equal(nextVersion(log('### Changed\n\n- A label.\n\n### Fixed\n\n- A bug.'), '0.11.1').version, '0.11.2')
})

test('something added makes a minor release, and the patch number starts again', () => {
  const next = nextVersion(log('### Added\n\n- A feature.\n\n### Fixed\n\n- A bug.'), '0.11.1')
  assert.equal(next.version, '0.12.0')
  assert.match(next.why, /Added/)
})

test('something removed is noticed too', () => {
  assert.equal(nextVersion(log('### Removed\n\n- An old switch.'), '0.11.1').version, '0.12.0')
})

test('the word Added in a sentence is not a heading', () => {
  assert.equal(nextVersion(log('### Fixed\n\n- Added tasks no longer vanish.'), '0.11.1').version, '0.11.2')
})

test('nothing to release is refused, not numbered', () => {
  assert.throws(() => nextVersion(log('Nothing yet.'), '0.11.1'), /nothing to release/)
})

test('the number worked out is the number the changelog is stamped with', () => {
  const text = log('### Added\n\n- A feature.')
  const { version } = nextVersion(text, '0.11.1')
  const stamped = release(text, version, '2026-11-13')
  assert.match(section(stamped, version), /A feature/)
  assert.equal(section(stamped, 'Unreleased'), 'Nothing yet.')
  assert.match(stamped, /^## \[0\.12\.0\] - 2026-11-13$/m)
})
