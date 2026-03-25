import { contextBridge, ipcRenderer } from 'electron'

export interface FileNode {
  name: string
  path: string
  type: 'file' | 'directory'
  children?: FileNode[]
}

export interface ElectronAPI {
  openFolder: () => Promise<string | null>
  readDir: (path: string) => Promise<FileNode[]>
  readFile: (path: string) => Promise<string | null>
  writeFile: (path: string, content: string) => Promise<boolean>
  createFile: (dirPath: string, name: string) => Promise<string | null>
  renameFile: (oldPath: string, newName: string) => Promise<string | null>
  deleteFile: (filePath: string) => Promise<boolean>
  moveFile: (srcPath: string, destDir: string) => Promise<string | null>
  createFolder: (parentDir: string, name: string) => Promise<string | null>
  onOpenFolderFromCli: (callback: (path: string) => void) => () => void
}

contextBridge.exposeInMainWorld('electronAPI', {
  openFolder: () => ipcRenderer.invoke('open-folder'),
  readDir: (path: string) => ipcRenderer.invoke('read-dir', path),
  readFile: (path: string) => ipcRenderer.invoke('read-file', path),
  writeFile: (path: string, content: string) => ipcRenderer.invoke('write-file', path, content),
  createFile: (dirPath: string, name: string) => ipcRenderer.invoke('create-file', dirPath, name),
  renameFile: (oldPath: string, newName: string) => ipcRenderer.invoke('rename-file', oldPath, newName),
  deleteFile: (filePath: string) => ipcRenderer.invoke('delete-file', filePath),
  moveFile: (srcPath: string, destDir: string) => ipcRenderer.invoke('move-file', srcPath, destDir),
  createFolder: (parentDir: string, name: string) => ipcRenderer.invoke('create-folder', parentDir, name),
  onOpenFolderFromCli: (callback: (path: string) => void) => {
    const handler = (_: Electron.IpcRendererEvent, path: string) => callback(path)
    ipcRenderer.on('open-folder-from-cli', handler)
    return () => ipcRenderer.removeListener('open-folder-from-cli', handler)
  },
} satisfies ElectronAPI)
