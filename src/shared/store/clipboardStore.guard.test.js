// 剪贴板列表多选锚点位移护栏测试
//
// removeItems 删除项后,多选锚点 selectionAnchorIndex 必须按被删索引相对
// 锚点的位置精确位移(被删索引在锚点之前→锚点前移 1;锚点自身被删→保持;
// 之后→不动)。粗粒度统一减 removedCount 会把「锚点之后删除」也误位移,
// 导致下一次 Shift 范围选择基于漂移锚点。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const storeSource = readFileSync(
  new URL('./clipboardStore.js', import.meta.url),
  'utf8',
);

// 拆分拼接避免自命中(本测试读自己源码,§10.4 陷阱)
const removedCountLiteral = ['removed', 'Count'].join('');

test('removeItems 锚点修正必须按被删索引相对位置逐位移,不得粗粒度减 removedCount', () => {
  const removeBody = storeSource.slice(
    storeSource.indexOf('removeItems(ids) {'),
    storeSource.indexOf('moveLoadedItem('),
  );
  // 必须存在逐索引位移判断(被删索引在锚点之前才前移)
  assert.ok(
    removeBody.includes('if (index < anchor)'),
    'removeItems 锚点修正必须按被删索引位置逐位移(锚点前删才前移)',
  );
  // 不得再粗粒度减 removedCount(会把锚点之后的删除也误位移)
  const coarse = `${'selectionAnchorIndex'} - ${removedCountLiteral}`;
  assert.ok(
    !removeBody.includes(coarse),
    'removeItems 不得再粗粒度减 removedCount(锚点漂移根因)',
  );
});
