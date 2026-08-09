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
    /Object\.keys\(defaultSettings\)\.forEach\(key =>/,
    'getAllSettings 应继续以 defaultSettings 作为保存字段集合'
  );
});
