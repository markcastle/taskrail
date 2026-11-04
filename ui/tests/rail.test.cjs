(async () => {
// Drives the browser build of the rail (in-memory backend) through the main
// gestures and checks the result. Run with the Vite dev server up on 5173.
const { chromium } = require('playwright')
const assert = require('node:assert/strict')

const shots = process.argv[2] ?? process.env.RAIL_SHOTS ?? null
const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 320, height: 900 }, deviceScaleFactor: 1 })
const errors = []
page.on('pageerror', (e) => errors.push(String(e)))
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()))

await page.goto('http://localhost:5173/')
await page.getByText('Top Priorities').waitFor()
if (shots) await page.screenshot({ path: `${shots}/01-initial.png` })

const titles = async (tone) => page.locator(`[data-testid="section-${tone}"] .card__title`).allTextContents()

// 1. Initial layout: up next band, fold with "3 more", active card first.
assert.equal(await page.getByTestId('upnext').count(), 1, 'up next band shown')
assert.deepEqual(await titles('priority'), ['Q1 board meeting', 'Prepare board deck', 'Reply to supplier', 'Book venue', 'Update docs'])
assert.match(await page.locator('.section__more').textContent(), /3 more/)
assert.deepEqual(await titles('progress'), ['Design dashboard', 'Review pull request'])
assert.equal(await page.locator('.card--active .card__title').textContent(), 'Design dashboard')

// 2. Expand the fold.
await page.locator('.section__more').click()
await page.getByText('Show fewer').waitFor()
assert.equal((await titles('priority')).length, 8, 'all todo items plus the event after expanding')
await page.getByText('Show fewer').click()
await page.locator('.section__more', { hasText: '3 more' }).waitFor()

// 3. Quick-add via the + button lands at the top of To Do.
await page.getByRole('button', { name: 'Add a task', exact: true }).click()
await page.getByLabel('New task title').fill('Call the bank')
await page.keyboard.press('Enter')
await page.getByText('Call the bank').waitFor()
assert.equal((await titles('priority'))[1], 'Call the bank', 'new task sits right after the imminent event')

// 4. Quick-add via the keyboard shortcut.
await page.keyboard.press('Control+Shift+Space')
await page.getByLabel('New task title').waitFor()
await page.keyboard.press('Escape')
await page.waitForTimeout(100)
assert.equal(await page.getByLabel('New task title').count(), 0, 'escape closes quick-add')

// 5. Tap a card to open it, Start it, it leads In Progress.
await page.getByText('Call the bank').click()
await page.getByRole('button', { name: 'Start', exact: true }).click()
await page.locator('[data-testid="section-progress"] .card__title', { hasText: 'Call the bank' }).waitFor()
// The active card stays the hero at the top; the newly started card comes next.
assert.deepEqual(await titles('progress'), ['Design dashboard', 'Call the bank', 'Review pull request'])
if (shots) await page.screenshot({ path: `${shots}/02-started.png` })

// 6. Make it active, then flick it sideways to complete it.
await page.getByText('Call the bank').click()
await page.getByRole('button', { name: 'Make active', exact: true }).click()
await page.waitForFunction(() => document.querySelector('.card--active .card__title')?.textContent === 'Call the bank')
const card = page.locator('[data-testid="section-progress"] .card__body', { hasText: 'Call the bank' })
const box = await card.boundingBox()
await page.mouse.move(box.x + 40, box.y + box.height / 2)
await page.mouse.down()
for (let i = 1; i <= 6; i++) await page.mouse.move(box.x + 40 + i * 30, box.y + box.height / 2)
await page.mouse.up()
await page.waitForFunction(() => !document.querySelector('[data-testid="section-progress"] .card__title')?.textContent.includes('Call the bank') ||
  [...document.querySelectorAll('[data-testid="section-progress"] .card__title')].every((n) => n.textContent !== 'Call the bank'))
assert.ok(!(await titles('progress')).includes('Call the bank'), 'flicked card left In Progress')
await page.locator('[data-testid="section-done"] .section__header').click()
await page.locator('[data-testid="section-done"] .card__title', { hasText: 'Call the bank' }).waitFor()
assert.equal(await page.locator('.card--active').count(), 0, 'completing the active card clears active')

// 7. Drag: move "Book venue" above "Prepare board deck".
const from = await page.locator('.card__body', { hasText: 'Book venue' }).boundingBox()
const to = await page.locator('.card__body', { hasText: 'Prepare board deck' }).boundingBox()
await page.mouse.move(from.x + 100, from.y + from.height / 2)
await page.mouse.down()
await page.waitForTimeout(220)
for (let i = 1; i <= 8; i++) {
  await page.mouse.move(from.x + 100, from.y + from.height / 2 + ((to.y - from.y - 4) * i) / 8)
  await page.waitForTimeout(16)
}
await page.mouse.up()
await page.waitForFunction(() => {
  const t = [...document.querySelectorAll('[data-testid="section-priority"] .card__title')].map((n) => n.textContent)
  return t.indexOf('Book venue') < t.indexOf('Prepare board deck')
})
const after = await titles('priority')
assert.ok(after.indexOf('Book venue') < after.indexOf('Prepare board deck'), `drag reordered: ${after.join(', ')}`)
if (shots) await page.screenshot({ path: `${shots}/03-reordered.png` })

// 7b. A drag that starts with a small sideways wobble and no hold must still work.
{
  const from2 = await page.locator('.card__body', { hasText: 'Update docs' }).boundingBox()
  const to2 = await page.locator('.card__body', { hasText: 'Prepare board deck' }).boundingBox()
  const x = from2.x + 100, y = from2.y + from2.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + 8, y + 1)
  for (let i = 1; i <= 8; i++) {
    await page.mouse.move(x + 8, y + ((to2.y - from2.y) * i) / 8)
    await page.waitForTimeout(16)
  }
  await page.mouse.up()
  await page.waitForFunction(() => {
    const t = [...document.querySelectorAll('[data-testid="section-priority"] .card__title')].map((n) => n.textContent)
    return t.indexOf('Update docs') < t.indexOf('Prepare board deck')
  })
}

// 8. Settings round trip. dnd-kit swallows the first click for a moment
// after a drop (so a drag never reads as a click); give that moment time to pass.
await page.waitForTimeout(400)
await page.getByRole('button', { name: 'Settings', exact: true }).click()
await page.getByText('Dock to').waitFor()
await page.getByLabel('Left').check()
await page.getByRole('button', { name: 'Save', exact: true }).click()
await page.getByRole('button', { name: 'Settings', exact: true }).click()
assert.equal(await page.getByLabel('Left').isChecked(), true, 'edge persisted in the backend')
await page.getByRole('button', { name: 'Cancel', exact: true }).click()

assert.deepEqual(errors, [], 'no console or page errors')
console.log('UI test passed')
await browser.close()
})().catch(async (e) => {
  console.error(e)
  try {
    const { chromium } = require('playwright')
    void chromium
  } catch {}
  process.exit(1)
})
