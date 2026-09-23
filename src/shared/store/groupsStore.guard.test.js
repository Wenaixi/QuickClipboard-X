// 分组默认色护栏:addGroup 默认色必须与后端/弹窗/全部组的 #dc2626 收敛,
// 否则任何非弹窗调用者不传色即存白色,与三方口径分叉落库白色。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const store = readFileSync(join(here, './groupsStore.js'), 'utf8');

test('addGroup 默认色必须与后端/弹窗一致(#dc2626,不得遗留 #ffffff 死默认)', () => {
  assert.match(
    store,
    /color = '#dc2626'/,
    'addGroup 默认色必须收敛到 #dc2626(与后端 DEFAULT_GROUP_COLOR/弹窗/全部组一致)',
  );
  assert.doesNotMatch(
    store,
    /color = '#ffffff'/,
    'addGroup 不得遗留 #ffffff 死默认(非弹窗调用者不传色即存白色)',
  );
});