import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, './Tooltip.jsx'), 'utf8');
// 剥行注释,避免注释字面误命中(§10.4 陷阱)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

test('Tooltip 生效朝向必须直接写入 state——不得用自幂等 setState 把状态更新优化掉', () => {
  const code = strip(source);
  assert.match(
    code,
    /setEffectivePlacement\(base\.placement\)/,
    '生效朝向必须直接写入 base.placement(不能是自幂等 prev===base 短路)',
  );
  assert.doesNotMatch(
    code,
    /setEffectivePlacement\(\(prev\) =>/,
    '禁止用 setState(prev => prev === base ? prev : base) 把翻转到一半的朝向卡住',
  );
});