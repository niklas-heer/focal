const { spawn } = require('node:child_process')
const path = require('node:path')
const waitOn = require('wait-on')
const { chromium } = require('playwright')

async function main() {
  const root = process.cwd()
  const filePath = path.join(root, 'README.md')
  const content = ['# Heading One','','> quoted line','> second quote line','','```ts','const value = 1','console.log(value)','```','','A [link](https://example.com) and **bold** text.'].join('\n')
  const vite = spawn(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['exec', 'vite', '--', '--host', '127.0.0.1', '--port', '4177'], { cwd: root, stdio: 'ignore', env: { ...process.env, NODE_ENV: 'development' } })
  let browser
  try {
    await waitOn({ resources: ['tcp:127.0.0.1:4177'], timeout: 20000 })
    browser = await chromium.launch({ headless: true })
    const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } })
    page.on('console', msg => console.log('console:', msg.type(), msg.text()))
    page.on('pageerror', err => console.log('pageerror:', err.message))
    await page.addInitScript(({ root, filePath, content }) => {
      window.electronAPI = {
        openFolder: async () => root,
        readDir: async () => [{ name: 'README.md', path: filePath, type: 'file' }],
        readFile: async (target) => target === filePath ? content : null,
        writeFile: async () => true,
        createFile: async () => null,
        renameFile: async () => null,
        deleteFile: async () => false,
        moveFile: async () => null,
        createFolder: async () => null,
        onOpenFolderFromCli: () => () => {},
      }
    }, { root, filePath, content })
    await page.goto('http://127.0.0.1:4177', { waitUntil: 'networkidle' })
    await page.getByRole('button', { name: 'Open a folder' }).click()
    await page.locator('.file-tree-item').filter({ hasText: 'README' }).click()
    await page.waitForSelector('.cm-content')
    await page.waitForTimeout(500)
  } finally {
    if (browser) await browser.close().catch(() => {})
    vite.kill('SIGTERM')
  }
}
main().catch(err => { console.error(err); process.exitCode = 1 })
