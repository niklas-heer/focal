const { spawn } = require('node:child_process')
const path = require('node:path')
const waitOn = require('wait-on')
const { chromium } = require('playwright')

async function main() {
  const root = process.cwd()
  const readmePath = path.join(root, 'README.md')
  const readme = Array.from({ length: 80 }, (_, index) => [
    `# Section ${index + 1}`,
    '',
    `This is **bold** copy with a [link](https://example.com/${index + 1}) and \`inline code\`.`,
    '',
    '- first item',
    '- second item',
    '',
    `## Subsection ${index + 1}`,
    '',
    'More text to keep the document flowing.',
    '',
  ].join('\n')).join('\n')
  const workspacePath = root
  const filePath = readmePath

  const vite = spawn(
    process.platform === 'win32' ? 'npm.cmd' : 'npm',
    ['exec', 'vite', '--', '--host', '127.0.0.1', '--port', '5173'],
    {
      cwd: root,
      stdio: 'ignore',
      env: { ...process.env, NODE_ENV: 'development' },
    }
  )

  let browser
  const cleanup = async () => {
    if (browser) await browser.close().catch(() => {})
    vite.kill('SIGTERM')
  }

  try {
    await waitOn({ resources: ['tcp:127.0.0.1:5173'], timeout: 20000 })

    browser = await chromium.launch({ headless: true })
    const page = await browser.newPage({ viewport: { width: 1600, height: 1200 } })

    await page.addInitScript(({ workspacePath, filePath, readme }) => {
      const fileTree = [
        {
          name: 'README.md',
          path: filePath,
          type: 'file',
        },
      ]

      window.electronAPI = {
        openFolder: async () => workspacePath,
        readDir: async () => fileTree,
        readFile: async (path) => path === filePath ? readme : null,
        writeFile: async () => true,
        createFile: async () => null,
        renameFile: async () => null,
        deleteFile: async () => false,
        moveFile: async () => null,
        createFolder: async () => null,
        onOpenFolderFromCli: () => () => {},
      }
    }, { workspacePath, filePath, readme })

    await page.goto('http://127.0.0.1:5173', { waitUntil: 'networkidle' })

    await page.getByRole('button', { name: 'Open a folder' }).click()
    await page.locator('.file-tree-item').filter({ hasText: 'README' }).click()
    await page.waitForSelector('.hybrid-live-editor .cm-content')
    await page.waitForTimeout(300)

    const countDecorations = () => page.evaluate(() => ({
      headings: document.querySelectorAll('.cm-md-h1, .cm-md-h2, .cm-md-h3').length,
      bold: document.querySelectorAll('.cm-md-bold').length,
      links: document.querySelectorAll('.cm-md-link-text').length,
    }))

    const before = await countDecorations()

    await page.locator('.editor-wrapper .cm-scroller').evaluate((el) => {
      el.scrollTop = Math.floor(el.scrollHeight * 0.65)
    })
    await page.waitForTimeout(300)

    const after = await countDecorations()

    console.log(JSON.stringify({ before, after }, null, 2))

    if (before.headings === 0 || before.bold === 0) {
      throw new Error('Initial render did not decorate markdown as expected')
    }

    if (after.headings === 0 || after.bold === 0) {
      throw new Error('Decorations disappeared after scrolling')
    }
  } finally {
    await cleanup()
  }
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
