import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../../..');
const settingsServiceSource = fs.readFileSync(
  path.join(root, 'src/shared/services/settingsService.js'),
  'utf8'
);
const settingsStoreSource = fs.readFileSync(
  path.join(root, 'src/shared/store/settingsStore.js'),
  'utf8'
);

const defaultSettingsBody = settingsServiceSource
  .match(/export const defaultSettings = \{([\s\S]*?)\n\}/)?.[1];
assert.ok(defaultSettingsBody, 'defaultSettings 定义必须存在');

const backendFields = [
  'hotkeysEnabled',
  'quickpasteWindowWidth',
  'quickpasteWindowHeight',
  'appFilterMode',
  'appFilterList',
  'edgeSnapEdge',
  'edgeSnapRatio',
  'edgeSnapMonitorId',
];

test('保存设置时后端字段仍属于 defaultSettings 序列化集合', () => {
  const currentSettings = Object.fromEntries(
    backendFields.map((field) => [field, `preserved-${field}`])
  );
  const payload = {};
  for (const key of Object.keys(currentSettings)) {
    if (new RegExp(`\\b${key}\\s*:`).test(defaultSettingsBody)) {
      payload[key] = currentSettings[key];
    }
  }

  assert.deepEqual(payload, currentSettings);
  assert.match(
    settingsStoreSource,
    /this\.backendSettings = \{ \...settings \}/,
    '加载时必须保存服务端原始设置快照'
  );
  assert.match(
    settingsStoreSource,
    /Object\.prototype\.hasOwnProperty\.call\(defaultSettings, key\)/,
    '加载时只能回填 defaultSettings 已声明的字段，未知字段不能覆盖 Store 内部状态或方法'
  );
  const loadStart = settingsStoreSource.indexOf('async loadSettings() {');
  const loadEnd = settingsStoreSource.indexOf('// 保存单个设置项');
  assert.ok(loadStart >= 0 && loadEnd > loadStart, 'loadSettings 函数边界必须存在');
  const loadSettingsBody = settingsStoreSource.slice(loadStart, loadEnd);
  assert.doesNotMatch(
    loadSettingsBody,
    /key in this/,
    '加载时不能再按 Store 属性名回填，避免未来字段命名冲突'
  );
  assert.match(
    settingsStoreSource,
    /const settings = \{ \...this\.backendSettings \}/,
    'getAllSettings 必须以原始后端设置为底稿，避免未知字段在前端保存时丢失'
  );
});

test('全部截图与 AI 字段在 Rust 模型与前端默认值三处同步且 UI 可编辑', () => {
  // 第五十轮审计：动态提取 model.rs 中全部 screenshot_*/ai_* 字段，逐一验证
  // ①前端 defaultSettings 声明同一 camelCase key；②设置 UI 各分节消费对应字段。
  const modelSource = fs.readFileSync(
    path.join(root, 'src-tauri/src/services/settings/model.rs'),
    'utf8'
  );
  const rustFields = [...modelSource.matchAll(/pub (screenshot_\w+|ai_\w+):/g)].map((m) => m[1]);
  assert.ok(rustFields.length >= 20, `截图与 AI 字段应不少于 20 个，实际 ${rustFields.length}`);
  const toCamel = (snake) => snake.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
  const missingInDefaults = rustFields
    .map(toCamel)
    .filter((camel) => !new RegExp(`\\b${camel}\\s*:`).test(defaultSettingsBody));
  assert.deepEqual(missingInDefaults, [], '每个 Rust 截图/AI 字段都必须在前端默认值声明');
  // 运行时隐私确认状态由 AI 截图路径的确认对话框自动置位并持久化，
  // 设置 UI 不暴露是合理设计（用户反悔时应由确认对话框重新询问，而非偏好开关）。
  const runtimeStateFields = new Set(['screenshotAiCloudConfirmed']);
  // 快捷键字段在 ShortcutsSection 消费。
  const shortcutsUi = fs.readFileSync(
    path.join(root, 'src/windows/settings/sections/ShortcutsSection.jsx'),
    'utf8'
  );
  for (const key of ['screenshotShortcut', 'screenshotQuickSaveShortcut', 'screenshotQuickPinShortcut', 'screenshotQuickOcrShortcut']) {
    assert.ok(shortcutsUi.includes(`settings.${key}`), `ShortcutsSection 必须消费 ${key}`);
  }
  // 截图功能字段在 ScreenshotSection 消费。
  const screenshotUi = fs.readFileSync(
    path.join(root, 'src/windows/settings/sections/ScreenshotSection.jsx'),
    'utf8'
  );
  for (const camel of rustFields.map(toCamel)) {
    if (runtimeStateFields.has(camel)) continue;
    if (['screenshotShortcut', 'screenshotQuickSaveShortcut', 'screenshotQuickPinShortcut', 'screenshotQuickOcrShortcut'].includes(camel)) continue;
    if (camel.startsWith('ai') && camel !== 'screenshotAiEnabled' && camel !== 'screenshotAiPrompt') continue;
    assert.ok(screenshotUi.includes(`settings.${camel}`), `ScreenshotSection 必须消费 ${camel}`);
  }
  // AI 翻译组在 TranslationSection 消费，AI 视觉配置（key/model/baseUrl）在 AIConfigSection 消费。
  const translationUi = fs.readFileSync(
    path.join(root, 'src/windows/settings/sections/TranslationSection.jsx'),
    'utf8'
  );
  for (const camel of rustFields.map(toCamel)) {
    if (!camel.startsWith('ai') || camel.startsWith('screenshotAi')) continue;
    if (['aiApiKey', 'aiModel', 'aiBaseUrl'].includes(camel)) continue;
    assert.ok(translationUi.includes(`settings.${camel}`), `TranslationSection 必须消费 ${camel}`);
  }
  const aiUi = fs.readFileSync(
    path.join(root, 'src/windows/settings/sections/AIConfigSection.jsx'),
    'utf8'
  );
  for (const key of ['aiApiKey', 'aiModel', 'aiBaseUrl']) {
    assert.ok(aiUi.includes(`settings.${key}`), `AIConfigSection 必须消费 ${key}`);
  }
});

test('AI 配置四字段在 defaultSettings 声明且设置 UI 消费同一组 key', () => {
  // AI 视觉识别配置必须在前端默认值、Rust model.rs 与设置页 UI 三处同步，
  // 任何一处遗漏都会导致配置保存后无法回读或 UI 无法编辑。
  const aiKeys = [
    'screenshotAiEnabled',
    'screenshotAiPrompt',
    'aiApiKey',
    'aiModel',
    'aiBaseUrl',
  ];
  for (const key of aiKeys) {
    assert.match(
      defaultSettingsBody,
      new RegExp(`\\b${key}\\s*:`),
      `defaultSettings 必须声明 ${key}`
    );
  }
  const modelSource = fs.readFileSync(
    path.join(root, 'src-tauri/src/services/settings/model.rs'),
    'utf8'
  );
  // Rust 端序列化字段与前端 camelCase key 一一对应。
  const rustPairs = {
    screenshotAiEnabled: 'screenshot_ai_enabled',
    screenshotAiPrompt: 'screenshot_ai_prompt',
    aiApiKey: 'ai_api_key',
    aiModel: 'ai_model',
    aiBaseUrl: 'ai_base_url',
  };
  for (const [frontKey, rustKey] of Object.entries(rustPairs)) {
    assert.ok(modelSource.includes(rustKey), `model.rs 必须包含 ${rustKey}`);
  }
  // 设置页 UI 必须消费 aiApiKey/aiModel/aiBaseUrl。
  const uiSource = fs.readFileSync(
    path.join(root, 'src/windows/settings/sections/AIConfigSection.jsx'),
    'utf8'
  );
  for (const key of ['aiApiKey', 'aiModel', 'aiBaseUrl']) {
    assert.ok(uiSource.includes(`settings.${key}`), `AIConfigSection 必须消费 ${key}`);
  }
});
