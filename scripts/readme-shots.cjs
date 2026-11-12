// The README's screenshots, taken from the browser build with its stand-in
// data, so they can be retaken whenever the rail changes its looks.
//
//   npm run ui:dev                                   (in one terminal)
//   node scripts/readme-shots.cjs docs/images        (in another)
//
// The clock is fixed and the locale is British, so the pictures come out the
// same whoever takes them and whenever.
(async () => {
const { chromium } = require('playwright')
const fs = require('node:fs')
const out = process.argv[2]
const browser = await chromium.launch()
const WHEN = new Date(2026, 10, 12, 9, 20, 0)

async function open(scheme, width, height, hash = '') {
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 2, colorScheme: scheme, locale: 'en-GB', timezoneId: 'Europe/London' })
  const page = await context.newPage()
  await page.clock.install({ time: WHEN })
  await page.goto('http://localhost:5173/')
  await page.evaluate(() => localStorage.clear())
  await page.reload()
  await page.getByTestId('section-priority').waitFor()
  await page.evaluate(async () => {
    const b = window.__taskrail
    await b.addDemoConnector()
    // The connected tasks go to the top of the day, and the one overdue task is dealt with.
    await b.moveTask('demo-tracker:T-202', 'todo', 0)
    await b.moveTask('demo-tracker:T-201', 'todo', 0)
    await b.complete('book-venue')
  })
  await page.locator('.card__trail').first().waitFor()
  if (hash) {
    await page.goto('http://localhost:5173/' + hash)
    await page.reload()
  }
  await page.mouse.move(width / 2, height - 4)
  await page.waitForTimeout(600)
  return { context, page }
}

// 1. The rail, light and dark.
for (const scheme of ['light', 'dark']) {
  const { context, page } = await open(scheme, 320, 900)
  await page.screenshot({ path: `${out}/rail-${scheme}.png` })
  await context.close()
}

// 2. A task opened: where it sits, and what can be done with it.
{
  const { context, page } = await open('light', 320, 900)
  await page.locator('[data-testid="card-demo-tracker:T-202"] .card__title').click()
  await page.waitForTimeout(400)
  await page.mouse.move(160, 896)
  const top = await page.locator('[data-testid="section-priority"]').boundingBox()
  const card = await page.locator('[data-testid="card-demo-tracker:T-202"]').boundingBox()
  await page.screenshot({ path: `${out}/details.png`, clip: { x: 0, y: top.y - 10, width: 320, height: card.y + card.height - top.y + 22 } })
  await context.close()
}

// 3. The pin, both ways.
const header = {}
for (const [name, unpin] of [['pinned', false], ['free', true]]) {
  const { context, page } = await open('light', 320, 300)
  if (unpin) { await page.getByTestId('pin').click(); await page.waitForTimeout(300) }
  await page.mouse.move(60, 250)
  header[name] = (await page.screenshot({ clip: { x: 0, y: 0, width: 320, height: 104 } })).toString('base64')
  await context.close()
}
{
  const sheet = await (await browser.newContext({ viewport: { width: 700, height: 200 }, deviceScaleFactor: 2 })).newPage()
  await sheet.setContent(`<style>html,body{margin:0;background:transparent}.row{display:flex;gap:28px;padding:16px 16px 22px}img{display:block;border-radius:12px;box-shadow:0 1px 2px rgba(15,20,32,.14),0 8px 22px rgba(15,20,32,.16)}</style>
    <div class="row"><img src="data:image/png;base64,${header.pinned}" width="320" height="104"><img src="data:image/png;base64,${header.free}" width="320" height="104"></div>`)
  const box = await sheet.locator('.row').boundingBox()
  await sheet.screenshot({ path: `${out}/pin.png`, clip: box, omitBackground: true })
}

// 4. Settings: the Rail page and the Connectors page.
{
  const { context, page } = await open('light', 760, 900, '#settings')
  await page.getByRole('heading', { name: 'Rail', exact: true }).waitFor()
  await page.waitForTimeout(400)
  // Down to the end of the Width row, so the picture stops on a row and not through one.
  const width = await page.getByRole('slider', { name: 'Width' }).boundingBox()
  await page.screenshot({ path: `${out}/settings-rail.png`, clip: { x: 0, y: 0, width: 760, height: Math.ceil(width.y + width.height + 34) } })
  await page.getByRole('button', { name: 'Connectors', exact: true }).click()
  await page.getByTestId('connector-demo-tracker').waitFor()
  await page.mouse.move(700, 690)
  await page.waitForTimeout(300)
  const height = Math.min(700, Math.ceil(await page.evaluate(() => { const m = document.querySelector('.settings-main'); return Math.max(...[...m.children].map((c) => c.getBoundingClientRect().bottom)) + 28 })))
  await page.screenshot({ path: `${out}/settings-connectors.png`, clip: { x: 0, y: 0, width: 760, height: Math.max(height, 420) } })
  await context.close()
}
await browser.close()
for (const f of fs.readdirSync(out).filter((f) => !f.startsWith('draft'))) console.log(f, fs.statSync(`${out}/${f}`).size)
})().catch((e) => { console.error(e); process.exit(1) })
