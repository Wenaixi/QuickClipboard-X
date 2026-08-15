import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const rootDir = path.resolve(__dirname, '..')

const npmCmd = process.platform === 'win32' ? 'npm.cmd' : 'npm'

let child = null
let interrupted = false

// 启动 Vite 构建，返回 Promise。信号中断时 reject 触发 finally 清理
function run(cwd, args) {
  return new Promise((resolve, reject) => {
    if (interrupted) {
      reject(new Error('中断'))
      return
    }
    child = spawn(npmCmd, args, {
      cwd,
      stdio: 'inherit',
      env: process.env,
      shell: true,
    })

    child.on('exit', (code) => {
      child = null
      if (interrupted) {
        reject(new Error('中断'))
        return
      }
      if (code === 0) resolve()
      else reject(new Error(`${args.join(' ')} 失败，退出码 ${code}`))
    })
  })
}

async function main() {
  await run(rootDir, ['run', 'build'])
}

process.on('SIGINT', () => {
  if (interrupted) return
  interrupted = true
  try { child?.kill('SIGINT'); } catch {}
})

process.on('SIGTERM', () => {
  if (interrupted) return
  interrupted = true
  try { child?.kill('SIGTERM'); } catch {}
})

main().catch((err) => {
  if (interrupted) {
    console.error('[build] 编译中断，退出码: 130')
  } else {
    console.error(err)
  }
  process.exit(interrupted ? 130 : 1)
})
