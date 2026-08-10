import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const readSource = () => readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');

test('EmojiTab 字体判断必须 await 异步 osVersion 链路', async () => {
  const body = await readSource();

  assert.equal(
    body.includes('getWindowsBuildNumber(osVersion())'),
    false,
    '不得同步直接使用 osVersion()'
  );
  assert.match(body, /const getWindowsFamily = async \(\) =>/);
  assert.match(body, /getWindowsBuildNumber\(await osVersion\(\)\)/);
  assert.match(body, /const shouldUseEmojiFallbackFont = async \(\) =>/);
  assert.match(body, /return target === await getWindowsFamily\(\)/);

  const effectStart = body.indexOf('useEffect(() => {', body.indexOf('const shouldUseEmojiFallbackFont'));
  const effectEnd = body.indexOf('}, []);', effectStart);
  const effect = body.slice(effectStart, effectEnd);
  assert.match(effect, /await shouldUseEmojiFallbackFont\(\)/);
  assert.match(effect, /await ensureEmojiFallbackFontLoaded\(\)/);
  assert.match(effect, /catch \(e\)/);
  assert.match(effect, /if \(!cancelled\) setUseEmojiFallbackFont\(true\)/);
});
