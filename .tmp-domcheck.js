const { spawn } = require('node:child_process')
const path = require('node:path')
const waitOn = require('wait-on')
const { chromium } = require('playwright')

async function main() {
  const root = process.cwd()
  const filePath = path.join(root, 'README.md')
  const content = [
    '# Heading One',
    '',
    '> quoted line',
    '> second quote line',
    '',
    '```ts',
    'const value = 1',
    'console.log(value)',
    '```',
    '',
    'A [link](https://example.com) and **bold** text.',
  ].join('\n')
  const vite = spawn(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['exec', 'vite', '--', '--host', '127.0.0.1', '--port', '4176'], { cwd: root, stdio: 'ignore', env: { ...process.env, NODE_ENV: 'development' } })
  let browser
  try {
    await waitOn({ resources: ['tcp:127.0.0.1:4176'], timeout: 20000 })
    browser = await chromium.launch({ headless: true })
    const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } })
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
    await page.goto('http://127.0.0.1:4176', { waitUntil: 'networkidle' })
    await page.getByRole('button', { name: 'Open a folder' }).click()
    await page.locator('.file-tree-item').filter({ hasText: 'README' }).click()
    await page.waitForSelector('.cm-content')
    await page.waitForTimeout(300)
    const info = await page.evaluate(() => ({
      headingCount: document.querySelectorAll('.cm-md-h1').length,
      headingChipCount: document.querySelectorAll('.cm-md-heading-chip').length,
      quoteMarkerCount: Array.from(document.querySelectorAll('.cm-line')).filter(el => el.textContent?.includes('> quoted')).length,
      hiddenMarkerCount: document.querySelectorAll('.cm-md-marker-hidden').length,
      codeChipCount: document.querySelectorAll('.cm-md-codeblock-chip').length,
      codeFenceVisibleCount: document.querySelectorAll('.cm-md-codeblock-fence-active, .cm-md-codeblock-fence-end-active').length,
      linkTextCount: document.querySelectorAll('.cm-md-link-text').length,
      boldCount: document.querySelectorAll('.cm-md-bold').length,
      text: document.querySelector('.editor-wrapper .cm-content')?.textContent,
    }))
    console.log(JSON.stringify(info, null, 2))
  } finally {
    if (browser) await browser.close().catch(() => {})
    vite.kill('SIGTERM')
  }
}
main().catch(err => { console.error(err); process.exitCode = 1 })
