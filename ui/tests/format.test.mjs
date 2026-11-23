// How a due date reads on the rail, and when it counts as late.
// `npm run ui:unit`. Each test sets the clock's zone itself, because the bug
// this guards against is about where midnight falls.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { dueWord } from '../src/format.ts'

const zone = (name) => (process.env.TZ = name)

test('a task due today is not late four minutes into the day', () => {
  zone('Europe/London')
  // Monday 23 November, 00:04. The tracker sent the date with no time.
  const due = '2026-11-23T00:00:00Z'
  assert.deepEqual(dueWord(due, new Date(2026, 10, 23, 0, 4)), { text: 'Due today', overdue: false })
  assert.deepEqual(dueWord(due, new Date(2026, 10, 23, 17, 30)), { text: 'Due today', overdue: false })
  assert.deepEqual(dueWord(due, new Date(2026, 10, 23, 23, 59)), { text: 'Due today', overdue: false })
})

test('it is late once the day is over', () => {
  zone('Europe/London')
  assert.deepEqual(dueWord('2026-11-23T00:00:00Z', new Date(2026, 10, 24, 0, 1)), { text: 'Overdue', overdue: true })
  assert.deepEqual(dueWord('2026-11-20T00:00:00Z', new Date(2026, 10, 23, 9, 0)), { text: 'Overdue', overdue: true })
})

test('a date with no time is the same day west of Greenwich', () => {
  zone('America/New_York')
  const due = '2026-11-23T00:00:00Z' // 19:00 on the 22nd in New York, read as a moment
  assert.deepEqual(dueWord(due, new Date(2026, 10, 22, 20, 0)), { text: 'Due tomorrow', overdue: false })
  assert.deepEqual(dueWord(due, new Date(2026, 10, 23, 9, 0)), { text: 'Due today', overdue: false })
  assert.deepEqual(dueWord(due, new Date(2026, 10, 24, 9, 0)), { text: 'Overdue', overdue: true })
})

test('midnight where the person is also means the day', () => {
  zone('Asia/Tokyo')
  const due = '2026-11-22T15:00:00Z' // midnight on the 23rd in Tokyo
  assert.deepEqual(dueWord(due, new Date(2026, 10, 23, 0, 4)), { text: 'Due today', overdue: false })
  assert.deepEqual(dueWord(due, new Date(2026, 10, 24, 0, 4)), { text: 'Overdue', overdue: true })
})

test('a task due at a time shows the time, and is late once it passes', () => {
  zone('Europe/London')
  const due = '2026-11-23T16:00:00Z'
  const before = dueWord(due, new Date(2026, 10, 23, 15, 0))
  assert.match(before.text, /^Due .*(16|4)[:.]00/)
  assert.equal(before.overdue, false)
  const after = dueWord(due, new Date(2026, 10, 23, 16, 1))
  assert.match(after.text, /^Due .*(16|4)[:.]00/)
  assert.equal(after.overdue, true)
  assert.deepEqual(dueWord(due, new Date(2026, 10, 24, 9, 0)), { text: 'Overdue', overdue: true })
})

test('later days are named and never late', () => {
  zone('Europe/London')
  const now = new Date(2026, 10, 23, 9, 0)
  assert.deepEqual(dueWord('2026-11-24T00:00:00Z', now), { text: 'Due tomorrow', overdue: false })
  const friday = dueWord('2026-11-27T00:00:00Z', now)
  assert.match(friday.text, /^Due /)
  assert.equal(friday.overdue, false)
  const nextMonth = dueWord('2026-12-14T00:00:00Z', now)
  assert.match(nextMonth.text, /^Due .*14/)
  assert.equal(nextMonth.overdue, false)
})
