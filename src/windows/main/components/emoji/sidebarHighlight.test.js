import { test } from 'node:test';
import assert from 'node:assert/strict';

// EmojiTab 侧栏高亮双状态收敛护栏
// background: standards #1 发现 activeCategory useState 与 activeCategoryRef
// 双状态来源,ref 是决策真值(enterSidebar/moveSidebarBy),state 仅高亮渲染用。

test('EmojiTab 侧栏高亮只用 ref 单一真值', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // 删 useState 双状态:activeCategory state 不得再出现
  assert.equal(
    /const \[activeCategory, setActiveCategory\] = useState/.test(body),
    false,
    '不应再声明 activeCategory useState'
  );
  assert.equal(
    body.includes('setActiveCategory'),
    false,
    '不应再调用 setActiveCategory'
  );
  // 保留 ref 真值 + 高亮决策函数
  assert.ok(body.includes('activeCategoryRef'), '应保留 activeCategoryRef');
  assert.ok(body.includes('updateSidebarHighlight'), '应保留 updateSidebarHighlight');
  // 高亮渲染用 ref 当前值 + fallback 第一项
  assert.ok(
    body.includes('activeCategoryRef.current'),
    '高亮应读 activeCategoryRef.current'
  );
  assert.ok(body.includes('isSidebarCategoryActive'), '高亮决策应走 isSidebarCategoryActive');
});

test('updateSidebarHighlight 必须 setSidebarHighlightTick 递增(tick 信号)', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const start = body.indexOf('const updateSidebarHighlight');
  assert.notEqual(start, -1, '缺 updateSidebarHighlight 定义');
  const next = body.indexOf('useEffect', start);
  const fnBody = body.slice(start, next === -1 ? body.length : next);
  // tick 是"ref 变更可见于渲染"的唯一信号:删掉 setSidebarHighlightTick 后
  // 高亮切换不再触发重渲,ref 改了 UI 不更新。此前六条断言全是形式存在性
  // 检查,删掉这一行测试仍全绿,不可证伪。
  assert.ok(
    /setSidebarHighlightTick\(tick\s*=>\s*tick\s*\+\s*1\)/.test(fnBody),
    'updateSidebarHighlight 必须 setSidebarHighlightTick(tick => tick + 1) 强制重渲'
  );
});
