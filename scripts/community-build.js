#!/usr/bin/env node
// 社区版构建：公共 manifest 已移除私有插件，保留无签名 updater 产物的兼容处理。
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import path from 'path';
import fs from 'fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.join(__dirname, '..');
const isDev = process.argv.includes('--dev');
const command = isDev ? 'dev' : 'build';
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');

function patchTauriConfForCommunity() {
    if (!fs.existsSync(tauriConfPath)) return () => {};

    const original = fs.readFileSync(tauriConfPath, 'utf8');
    let json;
    try {
        json = JSON.parse(original);
    } catch {
        return () => {};
    }

    if (!json.bundle || json.bundle.createUpdaterArtifacts !== true) return () => {};

    json.bundle.createUpdaterArtifacts = false;
    fs.writeFileSync(tauriConfPath, JSON.stringify(json, null, 2) + '\n', 'utf8');
    return () => fs.writeFileSync(tauriConfPath, original, 'utf8');
}

const args = ['run', 'tauri', '--', command];
console.log('[build] 版本: 社区版');
console.log('[build] 模式: ' + (isDev ? '开发' : '生产'));
console.log('[build] 执行: npm ' + args.join(' '));

let restored = false;
const restoreConfig = patchTauriConfForCommunity();
const restoreOnce = () => {
    if (restored) return;
    restored = true;
    try {
        restoreConfig();
    } catch (err) {
        console.error('[build] 还原 tauri.conf.json 失败:', err.message);
    }
};

let interrupted = false;
const child = spawn('npm', args, {
    stdio: 'inherit',
    cwd: rootDir,
    shell: true,
    env: { ...process.env, QC_COMMUNITY: '1' },
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
    restoreOnce();
    console.error('[build] 启动失败: ' + err.message);
    process.exit(1);
});

child.on('close', (code) => {
    restoreOnce();
    const exitCode = interrupted ? 130 : (code ?? 1);
    if (exitCode !== 0) {
        console.error('[build] 编译' + (interrupted ? '中断' : '失败') + '，退出码: ' + exitCode);
    } else {
        console.log('[build] 社区版编译完成');
    }
    process.exit(exitCode);
});
