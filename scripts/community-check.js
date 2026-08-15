#!/usr/bin/env node
// 社区版 Rust 检查/测试脚本。
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';

const rootDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const subcommand = process.argv[2];
const validCommands = {
  check: ['check', '--no-default-features'],
  clippy: ['clippy', '--no-default-features'],
  test: ['test', '--no-default-features'],
};

if (!validCommands[subcommand]) {
  console.error('用法: node scripts/community-check.js <check|clippy|test>');
  process.exit(1);
}

const cargoArgs = validCommands[subcommand];
console.log('[check] 执行: cargo ' + cargoArgs.join(' '));
let interrupted = false;
const child = spawn('cargo', cargoArgs, {
  stdio: 'inherit',
  cwd: path.join(rootDir, 'src-tauri'),
  shell: true,
});

process.on('SIGINT', () => {
  if (interrupted) return;
  interrupted = true;
  try { child.kill('SIGINT'); } catch {}
});

process.on('SIGTERM', () => {
  if (interrupted) return;
  interrupted = true;
  try { child.kill('SIGTERM'); } catch {}
});

child.on('error', (err) => {
  console.error('[check] 启动失败: ' + err.message);
  process.exit(1);
});

child.on('close', (code) => {
  const exitCode = interrupted ? 130 : (code ?? 1);
  if (exitCode !== 0) {
    console.error('[check] ' + subcommand + (interrupted ? ' 中断' : ' 失败') + '，退出码: ' + exitCode);
  } else {
    console.log('[check] ' + subcommand + ' 完成');
  }
  process.exit(exitCode);
});
