import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./contextMenu.js', import.meta.url), 'utf8');

test('贴图右键菜单不得含必失败的编辑入口', () => {
  // 后端 start_pin_edit_mode 是桩函数（硬编码返回不可用），前端入口点击必然失败。
  // 断言目标用拆分拼接：本测试读自己源码，完整字面量会自命中（§10.4 自指陷阱）。
  const editLabel = ['编辑', '贴图'].join('');
  const cmdName = ['start_pin_', 'edit_mode'].join('');
  assert.ok(!source.includes(editLabel), '未实现的后端功能不得保留菜单入口');
  assert.ok(!source.includes(cmdName), '不得调用桩命令');
  assert.ok(source.includes('case "copy"'), '复制入口必须保留');
});
