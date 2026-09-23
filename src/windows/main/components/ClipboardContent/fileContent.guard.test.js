// 文件内容组件挂键护栏:FileContent 的解析错误/无文件信息/文件计数文案
// 必须走语言包(en 用户可见),common.fileCount 双包必须齐全。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./FileContent.jsx', import.meta.url), 'utf8');
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
const code = strip(source);

test('FileContent 解析错误/无文件信息/文件计数必须走语言包键', () => {
  assert.ok(
    code.includes("t('clipboard.fileParseError')"),
    '文件数据解析错误必须走 clipboard.fileParseError 键',
  );
  assert.ok(
    code.includes("t('clipboard.noFileInfo')"),
    '无文件信息必须走 clipboard.noFileInfo 键',
  );
  assert.ok(
    code.includes("t('common.fileCount'"),
    '多文件计数必须走 common.fileCount 键(不得用不存在的 clipboard.fileCount)',
  );
  assert.ok(
    !code.includes("t('clipboard.fileCount'"),
    '不得引用不存在的 clipboard.fileCount 键(键路径错配会让 en 用户见中文兜底)',
  );
  assert.ok(
    !code.includes('>文件数据解析错误</div>'),
    '解析错误不得残留裸中文',
  );
  assert.ok(
    !code.includes('>无文件信息</div>'),
    '无文件信息不得残留裸中文',
  );
});

test('双语包必须含 clipboard.fileParseError/noFileInfo 与 common.fileCount', () => {
  const here = new URL('.', import.meta.url);
  // ClipboardContent → components → main → windows → src:四级回退
  const zh = JSON.parse(readFileSync(new URL('../../../../shared/locales/zh-CN.json', here), 'utf8'));
  const en = JSON.parse(readFileSync(new URL('../../../../shared/locales/en-US.json', here), 'utf8'));
  for (const [pack, label] of [[zh, 'zh'], [en, 'en']]) {
    assert.ok(pack.clipboard && typeof pack.clipboard.fileParseError === 'string', `${label} clipboard.fileParseError 必须存在`);
    assert.ok(pack.clipboard && typeof pack.clipboard.noFileInfo === 'string', `${label} clipboard.noFileInfo 必须存在`);
    assert.ok(pack.common && typeof pack.common.fileCount === 'string', `${label} common.fileCount 必须存在`);
  }
  assert.strictEqual(en.common.fileCount, 'files');
});