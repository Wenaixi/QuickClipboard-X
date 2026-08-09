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


// F10:imageRowCount(Virtuoso totalCount 给视觉布局)用 displayImageTotal 全量,
// 而 kbMove/executeCurrent/activateKb(键盘导航边界)用 displayImageItemsRef 已加载
// 数组。双口径是有意设计:用 max(loadedLen, total) 会让高亮落在 placeholder 上,
// Enter 静默。护栏:锁死 imageRowCount 必须用 displayImageTotal,不允许改用
// displayImageItemsRef(防止未来误改破坏 F1-4 边界语义)。
test('F10 imageRowCount 用 displayImageTotal(Virtuoso),与 kbMove 双口径是有意设计', async () => {
  const body = await readSource('./ImageLibraryTab.jsx');
  // 锚点:imageRowCount 末尾精确字面 }, [displayImageTotal, imageCols]);
  // (不能用 '});' 否则会越过 imageRowCount 命中下一个 useCallback 的 body 结束,
  // 把 displayImageItemsRef 引用卷进 fn,负向断言假红)
  const fnStart = body.indexOf('const imageRowCount = useMemo');
  assert.notEqual(fnStart, -1, '缺 const imageRowCount');
  const fnEndMarker = 'imageCols]);';
  const fnEnd = body.indexOf(fnEndMarker, fnStart);
  assert.notEqual(fnEnd, -1, '缺 imageRowCount 末尾锚点');
  const fn = body.slice(fnStart, fnEnd + fnEndMarker.length);
  assert.ok(fn.includes('displayImageTotal'), 'imageRowCount 应基于 displayImageTotal(Virtuoso 虚拟化需要全量行数)');
  assert.equal(fn.includes('displayImageItemsRef'), false, 'imageRowCount 不应基于 displayImageItemsRef,否则与 kbMove 边界重叠会让高亮落在 placeholder');
});
