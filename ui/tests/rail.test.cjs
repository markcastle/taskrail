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
await page.getByTestId('section-priority').waitFor()
if (shots) await page.screenshot({ path: `${shots}/01-initial.png` })

const titles = async (tone) => page.locator(`[data-testid="section-${tone}"] .card__title`).allTextContents()
const count = async (tone) => page.locator(`[data-testid="section-${tone}"] .section__count`).textContent()

// 1. Initial layout: the day in numbers, up next, the fold as a collapsed Later section, active card first.
assert.match(await page.getByTestId('day-progress').textContent(), /2 of 11 complete/)
assert.equal(await page.getByTestId('upnext').count(), 1, 'up next card shown')
assert.match(await page.getByTestId('upnext').textContent(), /Design dashboard/, 'the focused task is up next')
assert.deepEqual(await titles('priority'), ['Q1 board meeting', 'Prepare board deck', 'Reply to supplier', 'Book venue', 'Update docs'])
assert.equal(await count('later'), '3', 'three items behind the fold')
assert.equal(await page.locator('[data-testid="section-later"] .card').count(), 0, 'Later starts collapsed')
assert.deepEqual(await titles('progress'), ['Design dashboard', 'Review pull request'])
assert.equal(await page.locator('.card--active .card__title').textContent(), 'Design dashboard')

// 2. Open the fold, then switch to All tasks, where there is no fold at all.
await page.locator('[data-testid="section-later"] .section__header').click()
assert.deepEqual(await titles('later'), ['Plan next sprint', 'Release notes', 'Deploy to dev'])
await page.getByRole('radio', { name: 'All tasks' }).click()
assert.equal((await titles('priority')).length, 8, 'all todo items plus the event in All tasks')
assert.equal(await page.getByTestId('section-later').count(), 0, 'no Later section in All tasks')
await page.getByRole('radio', { name: 'Today' }).click()
await page.locator('[data-testid="section-later"]').waitFor()

// 2a. In progress folds at the same limit as Priorities. Drop the limit to 1 through the stand-in's settings.
await page.evaluate(async () => {
  const b = window.__taskrail
  const s = await b.getSettings()
  await b.updateSettings({ ...s, top_limit: 1 })
})
await page.locator('[data-testid="section-progress"] .section__more', { hasText: 'Show 1 more' }).waitFor()
assert.deepEqual(await titles('progress'), ['Design dashboard'], 'the active item stays visible above the fold')
await page.locator('[data-testid="section-progress"] .section__more').click()
assert.deepEqual(await titles('progress'), ['Design dashboard', 'Review pull request'])
await page.getByText('Show fewer').click()
await page.evaluate(async () => {
  const b = window.__taskrail
  const s = await b.getSettings()
  await b.updateSettings({ ...s, top_limit: 5 })
})
await page.locator('[data-testid="section-later"] .section__count', { hasText: '3' }).waitFor()

// 2a2. "One task past due." is a button: it narrows the rail to what is overdue, and back.
await page.getByRole('button', { name: 'One task past due.', exact: true }).click()
assert.deepEqual(await titles('priority'), ['Book venue'], 'only the overdue task shows')
assert.equal(await page.getByTestId('upnext').count(), 0, 'up next steps aside while filtering')
await page.getByRole('button', { name: 'Show everything', exact: true }).click()
await page.getByTestId('upnext').waitFor()
assert.equal((await titles('priority')).length, 5)

// 2b. Search filters every section and ignores the fold.
await page.getByRole('button', { name: 'Search', exact: true }).click()
await page.getByLabel('Search tasks').fill('deploy')
assert.deepEqual(await titles('priority'), ['Deploy to dev'], 'a hidden item is found by search')
await page.keyboard.press('Escape')
await page.locator('[data-testid="section-later"]').waitFor()

// 3. The + button puts the cursor in the add box; the new task lands at the top of To Do.
await page.getByRole('button', { name: 'Add a task', exact: true }).click()
await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'New task title')
await page.getByLabel('New task title').fill('Call the bank')
await page.keyboard.press('Enter')
await page.locator('.card__title', { hasText: 'Call the bank' }).waitFor()
assert.equal((await titles('priority'))[1], 'Call the bank', 'new task sits right after the imminent event')
assert.equal(await page.getByLabel('New task title').inputValue(), '', 'the add box clears after adding')

// 4. The keyboard shortcut focuses the add box; Escape lets go of it.
await page.locator('body').click({ position: { x: 5, y: 5 } })
await page.keyboard.press('Control+Shift+Space')
await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'New task title')
await page.keyboard.press('Escape')
await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') !== 'New task title')

// 4b. The circle completes a task straight from the list, and the day's count moves.
await page.getByRole('button', { name: 'Complete Reply to supplier', exact: true }).click()
await page.waitForFunction(() => !document.querySelector('[data-testid="section-priority"]')?.textContent.includes('Reply to supplier'))
assert.match(await page.getByTestId('day-progress').textContent(), /3 of 12 complete/)
await page.locator('[data-testid="section-done"] .section__header').click()
await page.locator('[data-testid="section-done"] .card__title', { hasText: 'Reply to supplier' }).waitFor()
await page.getByRole('button', { name: 'Reopen Reply to supplier', exact: true }).click()
await page.locator('[data-testid="section-priority"] .card__title', { hasText: 'Reply to supplier' }).waitFor()
await page.locator('[data-testid="section-done"] .section__header').click()

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
// dnd-kit swallows the first click after a drop, so give it a moment after the drag above.
await page.waitForTimeout(400)
await page.getByRole('button', { name: 'Settings', exact: true }).click()
await page.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
await page.getByRole('button', { name: '← Back to the rail', exact: true }).click()
await page.getByTestId('section-priority').waitFor()
await page.getByRole('button', { name: 'Settings', exact: true }).click()
await page.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
await page.keyboard.press('Escape')
await page.getByTestId('section-priority').waitFor()

// 8. Settings on their own page for development. In the browser that is a second tab on #settings
// sharing state through localStorage, the way the two windows share the backend in the app.
const settings = await context.newPage()
await settings.setViewportSize({ width: 820, height: 680 })
settings.on('pageerror', (e) => errors.push(String(e)))
settings.on('console', (m) => m.type() === 'error' && errors.push(m.text()))
await settings.goto('http://localhost:5173/#settings')
await settings.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
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
await settings.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
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
await page.locator('.card__source', { hasText: 'Demo tracker' }).first().waitFor()
assert.ok((await page.locator('.card__source').count()) >= 2, 'remote rows carry their source')
await page.locator('[data-testid="sync-status"]', { hasText: 'Up to date' }).waitFor()
if (shots) await page.screenshot({ path: `${shots}/05-rail-with-connector.png` })
// 9a. Pause: the card says so, a demo task refuses to move, resume brings it back.
await settings.getByRole('button', { name: 'Pause', exact: true }).click()
await settings.locator('[data-testid="connector-demo-tracker"] .connector__tag', { hasText: 'Paused' }).waitFor()
await page.locator('[data-testid="sync-status"]', { hasText: 'Demo tracker is paused' }).waitFor()
await page.locator('.card__title', { hasText: 'Migrate billing job' }).click()
await page.getByRole('button', { name: 'Complete', exact: true }).click()
await page.locator('.toast--error', { hasText: 'Demo tracker is paused' }).waitFor()
await settings.getByRole('button', { name: 'Resume', exact: true }).click()
await settings.locator('[data-testid="connector-demo-tracker"] .connector__tag').waitFor({ state: 'detached' })
await page.locator('[data-testid="sync-status"]', { hasText: 'Up to date' }).waitFor()
// 9b. Scope: the trail shows on a row; the panel's switch and ticks save and come back.
assert.match(await page.locator('[data-testid="card-demo-tracker:T-202"] .card__trail').textContent(), /Work › Now › Fix login timeout/)
await settings.getByRole('button', { name: 'Scope', exact: true }).click()
await settings.getByTestId('scope-demo-tracker').waitFor()
await settings.getByRole('switch', { name: 'Only tasks assigned to me' }).click()
await settings.getByRole('checkbox', { name: 'Work' }).click()
await settings.getByRole('button', { name: 'Close', exact: true }).click()
await settings.getByRole('button', { name: 'Scope', exact: true }).click()
await settings.getByTestId('scope-demo-tracker').waitFor()
assert.equal(await settings.getByRole('switch', { name: 'Only tasks assigned to me' }).getAttribute('aria-checked'), 'false', 'the switch stays off')
assert.equal(await settings.getByRole('checkbox', { name: /Work/ }).getAttribute('aria-checked'), 'true', 'the tick stays on')
await settings.getByRole('button', { name: 'Close', exact: true }).click()
await settings.getByRole('button', { name: 'Raw sample', exact: true }).click()
await settings.getByTestId('sample-demo-tracker').waitFor()
assert.match(await settings.getByLabel('Raw sample').inputValue(), /list_tickets/)
await settings.getByRole('button', { name: 'Close', exact: true }).click()
await settings.getByRole('button', { name: 'Blueprint', exact: true }).click()
await settings.getByTestId('blueprint-demo-tracker').waitFor()
assert.match(await settings.getByLabel('Blueprint JSON').inputValue(), /"match"/, 'the blueprint carries a fingerprint')
await settings.getByTestId('submit-blueprint').click()
await settings.locator('.toast', { hasText: 'submission form is open' }).waitFor()
await settings.getByRole('button', { name: 'Close', exact: true }).click()
await settings.getByRole('button', { name: /^Paste a profile/ }).click()
await settings.getByLabel('Connector profile JSON').fill('{"id": "demo-tracker", "name": "Dup"}')
await settings.getByRole('button', { name: 'Add connector', exact: true }).click()
await settings.locator('.toast--error', { hasText: 'already exists' }).waitFor()
await settings.getByTestId('connector-demo-tracker').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()
await page.waitForFunction(() => document.querySelectorAll('.card__source').length === 0)
await page.locator('[data-testid="sync-status"]', { hasText: 'Local tasks only' }).waitFor()

// 10. A server the registry knows needs no model at all, and no name: the blueprint supplies one.
await settings.getByRole('button', { name: /^Add connector Any system/ }).click()
await settings.getByLabel('Server URL').fill('https://blueprint.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
await settings.locator('.discovery .result--success', { hasText: 'Demo tracker connected.' }).waitFor()
assert.match(await settings.locator('.discovery .result--success').textContent(), /from the Demo tracker blueprint. No model was needed/)
await settings.getByTestId('connector-demo-tracker').waitFor()
// Undo takes it straight back out; discovering again puts it back.
await settings.getByTestId('undo-add').click()
await settings.getByText('Nothing connected yet.').waitFor()
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('connector-demo-tracker').waitFor()
await settings.getByTestId('done-add').click()
await settings.getByTestId('connector-demo-tracker').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 10c. A calendar: a bad address is refused with a reason, a good one lands today's events on the rail.
await settings.getByRole('button', { name: /^Add a calendar/ }).click()
await settings.getByLabel('Calendar address').fill('https://cal.example/broken.ics')
await settings.getByTestId('add-calendar-go').click()
await settings.locator('.result--error', { hasText: 'did not return a calendar' }).waitFor()
await settings.getByLabel('Calendar name').fill('Work')
await settings.getByLabel('Calendar address').fill('https://cal.example/work.ics')
await settings.getByTestId('add-calendar-go').click()
await settings.getByTestId('connector-work').waitFor()
assert.match(await settings.getByTestId('connector-work').textContent(), /Calendar/, 'the card says it is a calendar')
assert.equal(await settings.getByTestId('connector-work').getByRole('button', { name: 'Blueprint', exact: true }).count(), 0, 'no blueprint for a calendar')
await page.getByTestId('section-today').waitFor()
{
  const today = await titles('today')
  assert.ok(today.includes('Design review'), `today's event shows: ${today.join(', ')}`)
  assert.ok(!today.includes('Dentist'), "tomorrow's does not")
}
await settings.getByTestId('connector-work').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()
await page.waitForFunction(() => ![...document.querySelectorAll('[data-testid="section-today"] .card__title')].some((n) => n.textContent === 'Design review'))

// 10b. A pasted blueprint is used instead of the registry, and Paste profile knows one when it sees it.
await settings.getByRole('button', { name: /^Paste a profile/ }).click()
await settings.getByLabel('Connector profile JSON').fill('{"id": "x", "title": "Pasted thing", "match": {"tools": ["a"]}, "profile": {"list": {"tool": "a"}}}')
await settings.getByRole('button', { name: 'Add connector', exact: true }).click()
await settings.locator('.toast', { hasText: 'That is a blueprint, not a profile' }).waitFor()
await settings.getByRole('button', { name: /^Add connector Any system/ }).click()
await settings.getByLabel('Server name').fill('Pasted')
await settings.getByLabel('Server URL').fill('https://somewhere.example/mcp')
await settings.getByLabel('Blueprint to use').fill('{"id": "x", "title": "Pasted thing", "match": {"tools": ["a"]}, "profile": {"list": {"tool": "a"}}}')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
await settings.locator('.discovery .result--success', { hasText: 'Pasted thing blueprint' }).waitFor()
await settings.getByTestId('connector-pasted').waitFor()
await settings.getByTestId('done-add').click()
await settings.getByTestId('connector-pasted').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 10a. An unknown server with no key says so, then discovery through the model once a key exists.
await settings.getByRole('button', { name: /^Add connector Any system/ }).click()
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
await settings.getByRole('button', { name: /^Add connector Any system/ }).click()
await settings.getByLabel('Server name').fill('Tuesday')
await settings.getByLabel('Server URL').fill('https://tuesday.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
await settings.locator('.discovery .result--success', { hasText: 'Tuesday connected.' }).waitFor()
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
await settings.getByTestId('done-add').waitFor({ state: 'visible' })
await settings.setViewportSize({ width: 820, height: 680 })
await settings.getByTestId('connector-tuesday').waitFor()
await settings.getByTestId('done-add').click()
await settings.getByTestId('connector-tuesday').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 11. A server that wants a sign-in: discovery stops, Sign in runs the flow, discovery resumes.
await settings.getByRole('button', { name: /^Add connector Any system/ }).click()
await settings.getByLabel('Server name').fill('Jira')
await settings.getByLabel('Server URL').fill('https://oauth.example/mcp')
await settings.getByRole('button', { name: 'Discover', exact: true }).click()
await settings.getByTestId('sign-in-required').waitFor()
await settings.getByRole('button', { name: 'Sign in', exact: true }).click()
await settings.getByTestId('discovery-result').waitFor()
await settings.getByTestId('connector-jira').waitFor()
await settings.getByTestId('done-add').click()
await settings.getByTestId('connector-jira').getByRole('button', { name: 'Remove', exact: true }).click()
await settings.getByText('Nothing connected yet.').waitFor()

// 11b. Updates: a check finds the stand-in's newer release; the rail shows the banner; the switch persists.
await settings.getByRole('button', { name: 'Rail', exact: true }).click()
await settings.getByTestId('check-update').click()
await settings.getByText('Task Rail 0.2.0 is available.').waitFor()
await page.getByTestId('update-banner').waitFor()
assert.match(await page.getByTestId('update-banner').textContent(), /0\.2\.0/)
await page.getByRole('button', { name: 'Not now', exact: true }).click()
assert.equal(await page.getByTestId('update-banner').count(), 0, 'the banner can be put away')
await settings.getByRole('switch', { name: 'Update automatically' }).click()
await settings.locator('.saved--visible').waitFor()
await settings.reload()
await settings.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
assert.equal(await settings.getByRole('switch', { name: 'Update automatically' }).getAttribute('aria-checked'), 'true', 'auto-update persisted')

// 11c. Animation and sound are both switches. Motion off reaches the rail as a document attribute.
assert.equal(await page.evaluate(() => document.documentElement.dataset.motion), 'on', 'motion on by default')
assert.equal(await settings.getByRole('switch', { name: 'Sounds' }).getAttribute('aria-checked'), 'false', 'sounds off by default')
await settings.getByRole('switch', { name: 'Animation' }).click()
await page.waitForFunction(() => document.documentElement.dataset.motion === 'off')
await settings.getByRole('switch', { name: 'Sounds' }).click()
// The save is debounced; wait for it to reach the stand-in's storage before reloading.
await settings.waitForFunction(() => JSON.parse(localStorage.getItem('taskrail-dev-state') ?? '{}').settings?.sounds === true)
await settings.reload()
await settings.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
assert.equal(await settings.getByRole('switch', { name: 'Animation' }).getAttribute('aria-checked'), 'false', 'animation stays off')
assert.equal(await settings.getByRole('switch', { name: 'Sounds' }).getAttribute('aria-checked'), 'true', 'sounds stay on')
await settings.getByRole('switch', { name: 'Animation' }).click()
await page.waitForFunction(() => document.documentElement.dataset.motion === 'on')

// 11d. The rail's inner edge is a handle, on whichever side faces the desktop.
// The stand-in cannot move a window, but the width saved on release is the one
// dragged out: 60px inward from 320 is 260.
const edge = await page.evaluate(async () => (await window.__taskrail.getSettings()).edge)
const grip = page.getByTestId('grip')
const gripBox = await grip.boundingBox()
const inward = edge === 'right' ? 60 : -60
await page.mouse.move(gripBox.x + 3, 400)
await page.mouse.down()
await page.mouse.move(gripBox.x + 3 + inward, 400, { steps: 4 })
await page.mouse.up()
await page.waitForFunction(() => JSON.parse(localStorage.getItem('taskrail-dev-state') ?? '{}').settings?.width_px === 260)
await settings.reload()
await settings.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
assert.match(await settings.getByRole('slider', { name: 'Width' }).evaluate((el) => String(el.value)), /^260$/, 'the slider shows the dragged width')

// 11e. Auto-hide. Once the pointer has left, the rail tucks itself into a strip;
// a rest on the strip or a click brings it back; Escape puts it away.
await page.evaluate(async () => {
  const b = window.__taskrail
  await b.updateSettings({ ...(await b.getSettings()), auto_hide: true })
})
await page.locator('html').dispatchEvent('pointerleave')
await page.getByTestId('strip').waitFor()
await page.mouse.move(160, 450)
await page.getByTestId('section-priority').waitFor()
assert.equal(await page.getByTestId('strip').count(), 0, 'the rail is out')
await page.keyboard.press('Escape')
await page.getByTestId('strip').waitFor()
await page.getByTestId('strip').click()
await page.getByTestId('section-priority').waitFor()
await page.evaluate(async () => {
  const b = window.__taskrail
  await b.updateSettings({ ...(await b.getSettings()), auto_hide: false })
})
await page.locator('html').dispatchEvent('pointerleave')
await page.waitForTimeout(900)
assert.equal(await page.getByTestId('strip').count(), 0, 'with auto-hide off the rail stays out')

// 11f. About says who made it.
await settings.getByRole('button', { name: 'About', exact: true }).click()
await settings.getByTestId('about').waitFor()
assert.match(await settings.getByTestId('about').textContent(), /Developed by Mark Castle/)
assert.match(await settings.getByTestId('about').textContent(), /© 2026/)

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
