import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const htmlLayout = readFileSync(join(here, '../utils/htmlLayout.js'), 'utf8');
const htmlContent = readFileSync(
  join(here, '../../windows/main/components/ClipboardContent/HtmlContent.jsx'),
  'utf8',
);
const htmlPreview = readFileSync(join(here, '../../windows/preview/views/HtmlPreview.jsx'), 'utf8');
// 剥行注释,避免注释字面误命中(与 Rust 侧 §10.4 陷阱同构)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

test('htmlLayout.js 必须导出中和函数且对非根元素强制 position/float/transform 复位', () => {
  const code = strip(htmlLayout);
  assert.match(code, /export function applyHtmlLayout/, '必须导出 applyHtmlLayout');
  assert.match(code, /export function applyHtmlElementLayout/, '必须导出 applyHtmlElementLayout');
  assert.match(code, /position', 'static'/, '非根元素必须强制 position 复位 static');
  assert.match(code, /float', 'none'/, '非根元素必须强制 float 复位 none');
  assert.match(code, /transform', 'none'/, '非根元素必须强制 transform 复位 none');
});

test('HtmlContent 注入后必须调用 applyHtmlLayout 中和布局,外链图片必须 no-referrer + lazy', () => {
  const code = strip(htmlContent);
  assert.match(code, /import \{ applyHtmlLayout \} from '@shared\/utils\/htmlLayout'/, '必须复用共享布局中和');
  assert.ok(
    /applyHtmlLayout\(contentRef\.current\)/.test(code),
    '注入 innerHTML 后必须调用 applyHtmlLayout(contentRef.current) 中和布局',
  );
  assert.match(code, /referrerPolicy = 'no-referrer'/, '外链图片必须设置 no-referrer');
  assert.match(code, /img\.loading = 'lazy'/, '外链图片必须 lazy 懒加载');
});

test('HtmlPreview 必须复用共享 htmlLayout,不得保留本地重复定义', () => {
  const code = strip(htmlPreview);
  assert.match(
    code,
    /import \{[^}]*HTML_LAYOUT_WRAP[\s\S]*?\} from '@shared\/utils\/htmlLayout'/,
    'HtmlPreview 必须 import 共享 htmlLayout',
  );
  assert.match(
    code,
    /import \{[^}]*applyHtmlLayout[\s\S]*?\} from '@shared\/utils\/htmlLayout'/,
    'HtmlPreview 必须 import 共享 applyHtmlLayout',
  );
  assert.doesNotMatch(
    code,
    /function setImportantStyle\(style, property, value\)/,
    'HtmlPreview 不得保留本地 setImportantStyle 重复定义',
  );
  assert.doesNotMatch(
    code,
    /function applyHtmlElementLayout\(element, mode, isRoot = false\)/,
    'HtmlPreview 不得保留本地 applyHtmlElementLayout 重复定义',
  );
  assert.doesNotMatch(
    code,
    /const HTML_LAYOUT_WRAP = 'wrap'/,
    'HtmlPreview 不得保留本地 HTML_LAYOUT_WRAP 常量重复定义',
  );
});

test('i18n 插值必须开启 escapeValue,防止翻译值内嵌 HTML 注入', () => {
  const i18n = strip(readFileSync(join(here, '../i18n.js'), 'utf8'));
  assert.match(i18n, /escapeValue:\s*true/, 'i18next 插值必须开启 escapeValue');
  assert.doesNotMatch(i18n, /escapeValue:\s*false/, '禁止关闭插值转义');
});
