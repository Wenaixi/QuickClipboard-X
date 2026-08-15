import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const rootDir = path.resolve(__dirname, '..')

const isWin = process.platform === 'win32'

function run(cwd, commandLine) {
  if (interrupted) throw new Error('中断')
  const child = isWin
    ? spawn('cmd.exe', ['/d', '/s', '/c', commandLine], {
        cwd,
        stdio: 'inherit',
        env: process.env,
      })
    : spawn('sh', ['-lc', commandLine], {
        cwd,
        stdio: 'inherit',
        env: process.env,
      })

  child.on('error', (err) => {
    console.error(err)
    process.exit(1)
  })

  return child
}

let host = null
let interrupted = false

// 信号中断时 kill Vite 子进程，子进程 exit 后输出消息并退出
process.on('SIGINT', () => {
  if (interrupted) return
  interrupted = true
  try { host?.kill(); } catch {}
})

process.on('SIGTERM', () => {
  if (interrupted) return
  interrupted = true
  try { host?.kill(); } catch {}
})

async function main() {
  host = run(rootDir, 'npm run dev')

  host.on('exit', (code) => {
    if (interrupted) {
      console.error('[dev] 开发服务中断，退出码: 130')
      process.exit(130)
    } else if (code && code !== 0) {
      process.exit(code)
    }
  })
}

main().catch((err) => {
  if (interrupted) {
    console.error('[dev] 开发服务中断，退出码: 130')
  } else {
    console.error(err)
  }
  process.exit(interrupted ? 130 : 1)
})
