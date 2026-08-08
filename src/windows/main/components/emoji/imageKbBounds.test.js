import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// F1-4: ImageLibraryTab 键盘边界口径不一致。activateKb 用 displayImageTotal
// (全量)clamp,而 kbMove/executeCurrent 用 displayImageItemsRef(已加载数组);
// searchQuery 变化只过滤 displayImageItems,不清/不夹 kbImageIndex → 搜索缩小
// 结果集后高亮与执行失配(index 越界、Enter 静默无操作)。
// 修复:过滤计算处(resetImageLoadCache 调用点)重置/夹取 kbImageIndex。

test('F1-4 activateKb 与 kbMove 用同一已加载数组口径', async () => {
  const body = await readSource('./ImageLibraryTab.jsx');
  // activateKb 不再用 displayImageTotal clamp
  const impStart = body.indexOf('useImperativeHandle(ref');
  const impEnd = body.indexOf('handleSelectionMouseDown', impStart);
  const imp = body.slice(impStart, impEnd);
  assert.ok(imp.includes('displayImageItemsRef'), 'activateKb 应基于已加载数组 displayImageItemsRef');
  assert.equal(imp.includes('displayImageTotal'), false, 'activateKb 不应再直接用 displayImageTotal');
});

test('F1-4 searchQuery 过滤时重置/夹取 kbImageIndex(过滤计算处)', async () => {
  const body = await readSource('./ImageLibraryTab.jsx');
  // 过滤触发处:搜索词变化时重置 kbImageIndex(prevSearchQueryRef 对比守卫,
  // 与 currentGroup 切换同款清理)
  const fnStart = body.indexOf('const prevSearchQueryRef');
  const fnEnd = body.indexOf('const hasSearchQuery', fnStart);
  assert.notEqual(fnStart, -1, '缺 prevSearchQueryRef 搜索词对比');
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(fn.includes('kbImageIndexRef.current = -1'), '搜索词变化应同步清 ref');
  assert.ok(fn.includes('setKbImageIndex(-1)'), '搜索词变化应 setKbImageIndex(-1)');
});
