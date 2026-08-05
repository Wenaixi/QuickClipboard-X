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
