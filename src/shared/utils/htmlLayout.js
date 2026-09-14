// HTML 富文本布局中和共享工具
// 来源:HtmlPreview.jsx 的 applyHtmlElementLayout / applyHtmlLayout 抽出。
// 用途:剪贴板来源的 HTML 内容经 innerHTML 注入后,内联样式可伪造
// position/float/transform 位移覆盖宿主 UI 元素;对容器内所有元素逐层
// 重写这些关键布局属性为重要级别(important),并让表格/媒体自适应宽度,
// 保证富文本始终在容器内流式排版,不浮出覆盖其他元素(C1)。
// HtmlPreview(预览窗口)与 HtmlContent(主窗口列表)共用同一套中和逻辑。

export const HTML_LAYOUT_WRAP = 'wrap';
export const HTML_LAYOUT_NOWRAP = 'nowrap';

// 供 HtmlPreview 的测量布局路径复用——以 !important 覆盖内联样式。
export function setImportantStyle(style, property, value) {
  if (
    style.getPropertyValue(property) === value
    && style.getPropertyPriority(property) === 'important'
  ) {
    return;
  }

  style.setProperty(property, value, 'important');
}

const RESPONSIVE_LAYOUT_TAGS = new Set([
  'DIV',
  'P',
  'SECTION',
  'ARTICLE',
  'HEADER',
  'FOOTER',
  'MAIN',
  'ASIDE',
  'NAV',
  'BLOCKQUOTE',
  'LI',
  'UL',
  'OL',
  'SPAN',
  'STRONG',
  'EM',
  'B',
  'I',
  'U',
  'S',
  'SUB',
  'SUP',
  'SMALL',
  'FIGURE',
  'FIGCAPTION',
  'A',
]);

const MEDIA_TAGS = new Set(['IMG', 'VIDEO', 'CANVAS', 'SVG', 'IFRAME', 'OBJECT', 'EMBED']);
const PRESERVE_INTRINSIC_WIDTH_TAGS = new Set(['TABLE', 'IMG', 'VIDEO', 'CANVAS', 'SVG', 'IFRAME', 'OBJECT', 'EMBED']);

export function applyHtmlElementLayout(element, mode, isRoot = false) {
  if (!element || !element.style) {
    return;
  }

  const shouldWrap = mode !== HTML_LAYOUT_NOWRAP;
  const tagName = element.tagName;

  setImportantStyle(element.style, 'box-sizing', 'border-box');
  setImportantStyle(element.style, 'min-width', '0');
  setImportantStyle(element.style, 'white-space', shouldWrap ? 'normal' : 'pre');
  setImportantStyle(element.style, 'overflow-wrap', shouldWrap ? 'anywhere' : 'normal');
  setImportantStyle(element.style, 'word-break', shouldWrap ? 'break-word' : 'normal');

  if (tagName === 'PRE' || tagName === 'CODE') {
    setImportantStyle(element.style, 'white-space', shouldWrap ? 'pre-wrap' : 'pre');
    setImportantStyle(element.style, 'overflow-x', 'auto');
    setImportantStyle(element.style, 'width', 'auto');
    setImportantStyle(element.style, 'max-width', 'none');
  } else if (tagName === 'TABLE') {
    setImportantStyle(element.style, 'width', '100%');
    setImportantStyle(element.style, 'max-width', '100%');
    setImportantStyle(element.style, 'table-layout', 'fixed');
    setImportantStyle(element.style, 'border-collapse', 'collapse');
  } else if (tagName === 'IMG') {
    setImportantStyle(element.style, 'display', 'inline-block');
    setImportantStyle(element.style, 'width', 'auto');
    setImportantStyle(element.style, 'height', 'auto');
    setImportantStyle(element.style, 'max-width', 'none');
    setImportantStyle(element.style, 'object-fit', 'contain');
  } else if (MEDIA_TAGS.has(tagName)) {
    setImportantStyle(element.style, 'width', 'auto');
    setImportantStyle(element.style, 'height', 'auto');
    setImportantStyle(element.style, 'max-width', 'none');
  } else if (RESPONSIVE_LAYOUT_TAGS.has(tagName)) {
    setImportantStyle(element.style, 'width', 'auto');
    if (shouldWrap) {
      setImportantStyle(element.style, 'white-space', 'normal');
    }
  }

  if (!isRoot) {
    if (element.style.position && element.style.position !== 'static') {
      setImportantStyle(element.style, 'position', 'static');
      setImportantStyle(element.style, 'top', 'auto');
      setImportantStyle(element.style, 'right', 'auto');
      setImportantStyle(element.style, 'bottom', 'auto');
      setImportantStyle(element.style, 'left', 'auto');
    }

    if (element.style.float && element.style.float !== 'none') {
      setImportantStyle(element.style, 'float', 'none');
    }

    if (element.style.transform && element.style.transform !== 'none') {
      setImportantStyle(element.style, 'transform', 'none');
    }
  }

  if (!PRESERVE_INTRINSIC_WIDTH_TAGS.has(tagName)) {
    element.removeAttribute('width');
    element.removeAttribute('height');
  }
}

export function applyHtmlLayout(root, mode = HTML_LAYOUT_WRAP) {
  if (!root) {
    return;
  }

  applyHtmlElementLayout(root, mode, true);
  root.querySelectorAll('*').forEach((element) => {
    applyHtmlElementLayout(element, mode, false);
  });
}
