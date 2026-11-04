// Starts the Vite dev server, runs the browser test against it, shuts it
// down (including its children), and exits with the test's status.
// `npm run ui:test`. Fails fast if something else already holds the port.
import { spawn } from 'node:child_process'
import { setTimeout as sleep } from 'node:timers/promises'

const port = 5173
const url = `http://localhost:${port}/`

const reachable = async () => {
  try {
    return (await fetch(url)).ok
  } catch {
    return false
  }
}

if (await reachable()) {
  console.error(`Something is already serving ${url}. Stop it and run again.`)
  process.exit(2)
}

const vite = spawn('node', ['node_modules/vite/bin/vite.js', '--port', String(port), '--strictPort'], {
  stdio: ['ignore', 'ignore', 'inherit'],
  detached: true,
})
const stopVite = () => {
  try {
    process.kill(-vite.pid, 'SIGTERM')
  } catch {}
}
process.on('exit', stopVite)
process.on('SIGINT', () => process.exit(130))
process.on('SIGTERM', () => process.exit(143))

let code = 1
try {
  for (let i = 0; i < 60 && !(await reachable()); i++) await sleep(500)
  if (!(await reachable())) throw new Error('Vite did not start')
  code = await new Promise((resolve) => {
    const test = spawn(process.execPath, ['ui/tests/rail.test.cjs', ...process.argv.slice(2)], { stdio: 'inherit' })
    test.on('exit', (c) => resolve(c ?? 1))
  })
} catch (e) {
  console.error(e)
} finally {
  stopVite()
}
process.exit(code)
