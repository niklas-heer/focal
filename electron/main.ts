import { app, BrowserWindow, ipcMain, dialog } from 'electron'
import * as path from 'path'
import * as fs from 'fs'

const isDev = process.env.NODE_ENV === 'development' || !app.isPackaged

let mainWindow: BrowserWindow | null = null

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    titleBarStyle: 'hiddenInset',
    trafficLightPosition: { x: 16, y: 16 },
    backgroundColor: '#1a1b2e',
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.js'),
    },
    show: false,
  })

  if (isDev) {
    mainWindow.loadURL('http://localhost:5173')
    // mainWindow.webContents.openDevTools()
  } else {
    mainWindow.loadFile(path.join(__dirname, '../dist/index.html'))
  }

  mainWindow.once('ready-to-show', () => {
    mainWindow?.show()

    // Pass CLI folder argument to renderer
    const cliPath = process.argv.find((arg, i) => i > 1 && !arg.startsWith('--'))
    if (cliPath) {
      const resolved = path.resolve(cliPath)
      mainWindow?.webContents.send('open-folder-from-cli', resolved)
    }
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

// IPC: Open folder dialog
ipcMain.handle('open-folder', async () => {
  const result = await dialog.showOpenDialog(mainWindow!, {
    properties: ['openDirectory'],
  })
  if (result.canceled || result.filePaths.length === 0) return null
  return result.filePaths[0]
})

// IPC: Read directory recursively
interface FileNode {
  name: string
  path: string
  type: 'file' | 'directory'
  children?: FileNode[]
}

function readDirRecursive(dirPath: string, depth = 0): FileNode[] {
  if (depth > 5) return []
  try {
    const entries = fs.readdirSync(dirPath, { withFileTypes: true })
    const nodes: FileNode[] = []

    for (const entry of entries) {
      if (entry.name.startsWith('.')) continue
      const fullPath = path.join(dirPath, entry.name)

      if (entry.isDirectory()) {
        const children = readDirRecursive(fullPath, depth + 1)
        if (children.length > 0) {
          nodes.push({ name: entry.name, path: fullPath, type: 'directory', children })
        }
      } else if (entry.isFile() && /\.(md|markdown)$/.test(entry.name)) {
        nodes.push({ name: entry.name, path: fullPath, type: 'file' })
      }
    }

    return nodes.sort((a, b) => {
      if (a.type !== b.type) return a.type === 'directory' ? -1 : 1
      return a.name.localeCompare(b.name)
    })
  } catch {
    return []
  }
}

ipcMain.handle('read-dir', (_event, dirPath: string) => {
  return readDirRecursive(dirPath)
})

// IPC: Read file
ipcMain.handle('read-file', async (_event, filePath: string) => {
  try {
    return fs.readFileSync(filePath, 'utf-8')
  } catch {
    return null
  }
})

// IPC: Write file
ipcMain.handle('write-file', async (_event, filePath: string, content: string) => {
  try {
    fs.writeFileSync(filePath, content, 'utf-8')
    return true
  } catch {
    return false
  }
})

// IPC: Create new file
ipcMain.handle('create-file', async (_event, dirPath: string, name: string) => {
  const filePath = path.join(dirPath, name.endsWith('.md') ? name : `${name}.md`)
  try {
    fs.writeFileSync(filePath, '', 'utf-8')
    return filePath
  } catch {
    return null
  }
})

ipcMain.handle('rename-file', async (_event, oldPath: string, newName: string) => {
  const dir = path.dirname(oldPath)
  const newPath = path.join(dir, newName.endsWith('.md') ? newName : `${newName}.md`)
  try {
    fs.renameSync(oldPath, newPath)
    return newPath
  } catch {
    return null
  }
})

ipcMain.handle('delete-file', async (_event, filePath: string) => {
  try {
    fs.rmSync(filePath, { recursive: true })
    return true
  } catch {
    return false
  }
})

ipcMain.handle('move-file', async (_event, srcPath: string, destDir: string) => {
  const name = path.basename(srcPath)
  const destPath = path.join(destDir, name)
  try {
    fs.renameSync(srcPath, destPath)
    return destPath
  } catch {
    return null
  }
})

ipcMain.handle('create-folder', async (_event, parentDir: string, name: string) => {
  const folderPath = path.join(parentDir, name)
  try {
    fs.mkdirSync(folderPath, { recursive: true })
    return folderPath
  } catch {
    return null
  }
})

app.whenReady().then(() => {
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
