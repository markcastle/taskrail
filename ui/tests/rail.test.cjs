(async () => {
// Drives the browser build of the rail (in-memory backend) through the main
// gestures and checks the result. Run with the Vite dev server up on 5173.
const { chromium } = require('playwright')
const assert = require('node:assert/strict')

const shots = process.argv[2] ?? process.env.RAIL_SHOTS ?? null
const browser = await chromium.launch()
const context = await browser.newContext({ viewport: { width: 320, height: 900 }, deviceScaleFactor: 1 })
const page = await context.newPage()
const errors = []
page.on('pageerror', (e) => errors.push(String(e)))
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()))

await page.goto('http://localhost:5173/')
await page.evaluate(() => localStorage.clear())
await page.reload()
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

// 8a. Settings also open inside the rail itself: the gear shows them, Back returns.
await page.getByRole('button', { name: 'Settings', exact: true }).click()
await page.getByRole('heading', { name: 'Rail' }).waitFor()
await page.getByRole('button', { name: '← Back to the rail', exact: true }).click()
await page.getByTestId('section-priority').waitFor()
await page.getByRole('button', { name: 'Settings', exact: true }).click()
await page.getByRole('heading', { name: 'Rail' }).waitFor()
await page.keyboard.press('Escape')
await page.getByTestId('section-priority').waitFor()

// 8. Settings on their own page for development. In the browser that is a second tab on #settings
// sharing state through localStorage, the way the two windows share the backend in the app.
const settings = await context.newPage()
await settings.setViewportSize({ width: 820, height: 680 })
settings.on('pageerror', (e) => errors.push(String(e)))
settings.on('console', (m) => m.type() === 'error' && errors.push(m.text()))
await settings.goto('http://localhost:5173/#settings')
await settings.getByRole('heading', { name: 'Rail' }).waitFor()
// Controls save themselves: no Save button anywhere on this page.
assert.equal(await settings.getByRole('button', { name: 'Save', exact: true }).count(), 0)
await settings.getByRole('radio', { name: 'Left edge' }).click()
await settings.getByRole('radio', { name: 'Dark' }).click()
await settings.getByRole('radio', { name: /Display 2/ }).click()
await settings.getByLabel('Top priorities shown').fill('7')
await settings.locator('.saved--visible').waitFor()
await settings.waitForFunction(() => document.documentElement.dataset.theme === 'dark')
await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark')
await settings.reload()
await settings.getByRole('heading', { name: 'Rail' }).waitFor()
assert.equal(await settings.getByRole('radio', { name: 'Left edge' }).getAttribute('aria-checked'), 'true', 'edge persisted in the backend')
assert.equal(await settings.getByRole('radio', { name: 'Dark' }).getAttribute('aria-checked'), 'true', 'theme persisted in the backend')
assert.equal(await settings.getByLabel('Top priorities shown').inputValue(), '7', 'slider value persisted')
assert.equal(await settings.getByRole('radio', { name: /Display 2/ }).getAttribute('aria-checked'), 'true', 'monitor choice persisted')
if (shots) await settings.screenshot({ path: `${shots}/04-settings-rail.png` })

// 9. Connectors: add the demo tracker on the settings page, see its cards arrive on the rail with a source mark, remove it.
await settings.getByRole('button', { name: 'Connectors', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()
await settings.getByRole('button', { name: /^Add demo tracker/ }).click()
await settings.getByTestId('connector-demo-tracker').waitFor()
assert.equal(await settings.getByTestId('toast').getAttribute('class'), 'toast toast--success', 'a success is green, not red')
await page.locator('.card__source', { hasText: 'demo-tracker' }).first().waitFor()
assert.ok((await page.locator('.card__source').count()) >= 2, 'remote cards carry their source')
if (shots) await page.screenshot({ path: `${shots}/05-rail-with-connector.png` })
await settings.getByRole('button', { name: /^Paste profile/ }).click()
await settings.getByLabel('Connector profile JSON').fill('{"id": "demo-tracker", "name": "Dup"}')
await settings.getByRole('button', { name: 'Add connector', exact: true }).click()
await settings.locator('.toast--error', { hasText: 'already exists' }).waitFor()
await settings.getByTestId('connector-demo-tracker').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()
await page.waitForFunction(() => document.querySelectorAll('.card__source').length === 0)

// 10. Model settings, then discovery of a new server through the model.
await settings.getByRole('button', { name: /^Add server/ }).click()
await settings.getByLabel('Server name').fill('Tuesday')
await settings.getByLabel('Server URL').fill('https://tuesday.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.locator('.result--error', { hasText: 'no API key' }).waitFor()
await settings.getByRole('button', { name: 'Model', exact: true }).click()
await settings.getByRole('radio', { name: 'OpenRouter' }).click()
await settings.locator('.saved--visible').waitFor()
await settings.getByLabel('Model name').fill('anthropic/claude-opus-5.5')
await settings.getByLabel('Model name').press('Enter')
await settings.getByLabel('API key').fill('or-test-key')
await settings.getByLabel('API key').press('Enter')
await settings.locator('.toast--success', { hasText: 'Key saved' }).waitFor()
await settings.getByTestId('test-model').click()
await settings.locator('.spinner', { hasText: 'Testing' }).waitFor()
await settings.locator('.result--success', { hasText: 'Connected' }).waitFor()
assert.match(await settings.locator('.result--success').textContent(), /replied: Hello/)
if (shots) await settings.screenshot({ path: `${shots}/06-settings-model.png` })
await settings.getByRole('button', { name: 'Connectors', exact: true }).click()
await settings.getByRole('button', { name: /^Add server/ }).click()
await settings.getByLabel('Server name').fill('Tuesday')
await settings.getByLabel('Server URL').fill('https://tuesday.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
assert.match(await settings.locator('.discovery .result--success').textContent(), /Found 2 items in 1 round/)
assert.match(await settings.getByLabel('Discovered profile JSON').inputValue(), /"id": "tuesday"/)
if (shots) await settings.screenshot({ path: `${shots}/07-settings-discovery.png` })
// A long page scrolls inside the main pane: the window itself never grows past the screen.
await settings.setViewportSize({ width: 760, height: 420 })
const scroll = await settings.locator('.settings-main').evaluate((el) => ({
  canScroll: el.scrollHeight > el.clientHeight,
  fits: el.getBoundingClientRect().bottom <= window.innerHeight + 1,
}))
assert.ok(scroll.canScroll, 'the settings pane has more content than fits, so it must scroll')
assert.ok(scroll.fits, 'the settings pane ends at the bottom of the window instead of running past it')
await settings.locator('.settings-main').evaluate((el) => el.scrollTo(0, el.scrollHeight))
await settings.getByRole('button', { name: 'Add connector', exact: true }).waitFor({ state: 'visible' })
await settings.setViewportSize({ width: 820, height: 680 })
await settings.getByRole('button', { name: 'Add connector', exact: true }).click()
await settings.getByTestId('connector-tuesday').waitFor()
await settings.getByTestId('connector-tuesday').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 11. A server that wants a sign-in: discovery stops, Sign in runs the flow, discovery resumes.
await settings.getByRole('button', { name: /^Add server/ }).click()
await settings.getByLabel('Server name').fill('Jira')
await settings.getByLabel('Server URL').fill('https://oauth.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('sign-in-required').waitFor()
await settings.getByRole('button', { name: 'Sign in', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
await settings.getByRole('button', { name: 'Add connector', exact: true }).click()
await settings.getByTestId('connector-jira').waitFor()
await settings.getByTestId('connector-jira').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 12. The Logs page shows the stand-in's log, including this page's own start line.
await settings.getByRole('button', { name: 'Logs', exact: true }).click()
await settings.getByTestId('log').waitFor()
assert.match(await settings.getByTestId('log').textContent(), /browser stand-in started/)

assert.deepEqual(errors, [], 'no console or page errors')
console.log('UI test passed')
await browser.close()
})().catch((e) => {
  console.error(e)
  process.exit(1)
})
