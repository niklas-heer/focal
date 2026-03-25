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

declare global {
  interface Window {
    electronAPI: ElectronAPI
  }
}
