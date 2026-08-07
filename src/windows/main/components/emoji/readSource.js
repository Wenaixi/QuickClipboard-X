import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// 测试样板收敛:读源文件 + 剥行注释。
// 各源码护栏测试(emoji 目录)原本各自内联"readFile + split + filter + join",
// 合计 30+ 处重复,收敛为本 helper 一处实现。
// 剥注释原因:护栏断言匹配函数体时,行注释里的同名标识符(如 '// EmojiTab 请求')
// 会污染 indexOf/slice 锚点与 includes 断言,统一剥掉避免假绿/误伤。

const here = path.dirname(fileURLToPath(import.meta.url));

/** 读 emoji 目录下的源文件(相对本文件所在目录),返回剥掉行注释后的源码 */
export async function readSource(relPath) {
  const src = await readFile(path.join(here, relPath), 'utf8');
  return src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
}

/** 读 emoji 目录下源文件的原始文本(不剥注释,供正则匹配 import 等场景) */
export async function readSourceRaw(relPath) {
  return readFile(path.join(here, relPath), 'utf8');
}
