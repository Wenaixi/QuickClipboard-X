import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./WebdavSection.jsx', import.meta.url), 'utf8');

test('WebDAV 设置节提供上传目标选择并保存 uploadTargetId', () => {
  // 上传目标（R5 截图「上传」动作消费）必须可在此保存：选择控件绑定
  // uploadTargetId 且包含 webdav 选项（与后端 target_for 派发一致）。
  assert.ok(source.includes('uploadTargetId'));
  assert.ok(source.includes("onSettingChange('uploadTargetId'"));
  assert.ok(source.includes("value: 'webdav'"));
});

test('WebDAV 设置节既有字段通过 update 保存', () => {
  const fields = [
    'webdavEnabled',
    'webdavUrl',
    'webdavUsername',
    'webdavRootPath',
    'webdavAutoPullOnWindowShow',
    'webdavAutoPush',
    'webdavAutoPull',
  ];
  for (const field of fields) {
    const call = "update('" + field + "'";
    assert.ok(
      source.includes(call),
      field + ' 必须有保存接线，不得是无消费的死设置',
    );
  }
});