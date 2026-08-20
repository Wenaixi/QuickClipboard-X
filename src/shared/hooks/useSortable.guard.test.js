import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, 'useSortable.js'), 'utf8');
// 剥行注释,避免注释字面误命中(与 Rust 侧 §10.4 陷阱同构)
const code = source
  .split('\n')
  .filter((line) => !line.trimStart().startsWith('//'))
  .join('\n');

test('拖拽激活阈值 distance 提高到 8px,减少日常点击被误判为拖拽', () => {
  assert.match(code, /distance:\s*8,/, 'activationConstraint.distance 必须为 8px');
  assert.doesNotMatch(code, /distance:\s*3,/, '禁止回退到 3px(手抖 3-5px 即触发拖拽,吞掉 item 点击)');
});

test('拖拽传感器必须尊重 data-drag-ignore 标记(滚动条/工具条区域不参与拖拽)', () => {
  assert.match(code, /dataset\.dragIgnore\s*===?\s*'true'/, 'shouldHandleDrag 必须检查 data-drag-ignore');
});
