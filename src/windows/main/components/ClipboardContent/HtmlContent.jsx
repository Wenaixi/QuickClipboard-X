import { useEffect, useRef } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { sanitizeHTML } from '@shared/utils/htmlProcessor';
import { invoke } from '@tauri-apps/api/core';
import { highlightHtmlContent, clearHighlights, scrollToFirstHighlight } from '@shared/utils/highlightText';
import { applyHtmlLayout } from '@shared/utils/htmlLayout';

const PLACEHOLDER_SRC = 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTAwIiBoZWlnaHQ9IjEwMCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cmVjdCB3aWR0aD0iMTAwIiBoZWlnaHQ9IjEwMCIgZmlsbD0iI2YwZjBmMCIvPjwvc3ZnPg==';
const ERROR_SRC = 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTAwIiBoZWlnaHQ9IjEwMCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cmVjdCB3aWR0aD0iMTAwIiBoZWlnaHQ9IjEwMCIgZmlsbD0iI2ZmZWJlZSIvPjx0ZXh0IHg9IjUwJSIgeT0iNTAlIiBmb250LXNpemU9IjEyIiBmaWxsPSIjYzYyODI4IiB0ZXh0LWFuY2hvcj0ibWlkZGxlIiBkeT0iLjNlbSI+5Zu+54mH5Yqg6L295aSx6LSlPC90ZXh0Pjwvc3ZnPg==';
// 图片 ID 白名单:与后端 is_valid_image_id 同语义。data-image-id 直接拼
// `${dataDir}/clipboard_images/{id}.png` 调 convertFileSrc,恶意 `..`/
// 绝对路径可越出图片目录读写 appdata 内任意文件,必须拼路径前校验。
const IMAGE_ID_WHITELIST = /^[A-Za-z0-9_-]{1,128}$/;

// HTML 富文本内容组件
function HtmlContent({
  htmlContent,
  lineClampClass,
  searchKeyword,
  rowHeight = 'medium',
  autoRowMaxLines = 18,
  maxContentHeightPx
}) {
  const contentRef = useRef(null);
  const processedRef = useRef(null);
  const hasScrolledRef = useRef(false);
  const prevKeywordRef = useRef('');

  useEffect(() => {
    if (!contentRef.current || processedRef.current === htmlContent) return;
    processedRef.current = htmlContent;
    const cleanHTML = sanitizeHTML(htmlContent);
    contentRef.current.innerHTML = cleanHTML;
    // C1:注入后立即中和布局——剪贴板来源的 HTML 内联样式可伪造
    // position/float/transform 覆盖宿主 UI 元素,统一重写为流式布局。
    applyHtmlLayout(contentRef.current);
    const images = contentRef.current.querySelectorAll('img');
    images.forEach(img => {
      const imageId = img.getAttribute('data-image-id');
      const src = img.getAttribute('src');

      // C2:非本地图片(无 data-image-id / 非 image-id: 前缀)的远程 src
      // 会被 WebView 自动拉取,统一 referrerPolicy 防泄露来源站 + lazy 懒加载
      // 避免列表一次性并发拉取大量外链图片拖慢窗口。
      if (!imageId && !(src && src.startsWith('image-id:'))) {
        img.referrerPolicy = 'no-referrer';
        img.loading = 'lazy';
      }

      // 优先使用 data-image-id
      if (imageId) {
        const originalSrc = img.src;
        // 白名单不合法直接回退原 src,不得拼路径调 convertFileSrc。
        if (!IMAGE_ID_WHITELIST.test(imageId)) {
          img.classList.add('html-image-pending');
          img.src = ERROR_SRC;
          img.classList.remove('html-image-pending');
          return;
        }
        img.src = PLACEHOLDER_SRC;
        img.classList.add('html-image-pending');
        invoke('get_data_directory').then(dataDir => {
          const filePath = `${dataDir}/clipboard_images/${imageId}.png`;
          const assetUrl = convertFileSrc(filePath, 'asset');
          img.src = assetUrl;
          img.classList.remove('html-image-pending');
        }).catch(error => {
          console.error('加载本地图片失败，恢复原始src:', error, 'imageId:', imageId);
          img.src = originalSrc;
          img.classList.remove('html-image-pending');
        });
      } else if (src && src.startsWith('image-id:')) {
        const legacyImageId = src.substring(9);
        // 历史 image-id: 前缀同款白名单,不合法直接置失败占位。
        if (!IMAGE_ID_WHITELIST.test(legacyImageId)) {
          img.classList.add('html-image-pending');
          img.src = ERROR_SRC;
          img.alt = '图片加载失败';
          img.classList.remove('html-image-pending');
          return;
        }
        img.src = PLACEHOLDER_SRC;
        img.classList.add('html-image-pending');
        invoke('get_data_directory').then(dataDir => {
          const filePath = `${dataDir}/clipboard_images/${legacyImageId}.png`;
          const assetUrl = convertFileSrc(filePath, 'asset');
          img.src = assetUrl;
          img.classList.remove('html-image-pending');
        }).catch(error => {
          console.error('加载 HTML 图片失败:', error, 'imageId:', legacyImageId);
          img.src = ERROR_SRC;
          img.alt = '图片加载失败';
          img.classList.remove('html-image-pending');
        });
      }
    });
  }, [htmlContent]);

  // 处理搜索高亮
  useEffect(() => {
    if (!contentRef.current) return;

    if (searchKeyword !== prevKeywordRef.current) {
      hasScrolledRef.current = false;
      prevKeywordRef.current = searchKeyword;
    }
    
    if (searchKeyword) {
      clearHighlights(contentRef.current);
      highlightHtmlContent(contentRef.current, searchKeyword);

      if (!hasScrolledRef.current) {
        requestAnimationFrame(() => {
          if (scrollToFirstHighlight(contentRef.current)) {
            hasScrolledRef.current = true;
          }
        });
      }
    } else {
      clearHighlights(contentRef.current);
    }
  }, [searchKeyword, htmlContent]);

  const clampClass = searchKeyword || rowHeight === 'auto' ? '' : lineClampClass;
  const autoClampStyle = !searchKeyword && rowHeight === 'auto'
    ? {
        display: '-webkit-box',
        WebkitLineClamp: autoRowMaxLines,
        WebkitBoxOrient: 'vertical'
      }
    : undefined;

  return <div ref={contentRef} className={`text-sm text-qc-fg leading-relaxed html-content overflow-hidden scrollbar-thin scrollbar-thumb-qc-border-strong scrollbar-track-transparent ${clampClass}`} style={{
    ...autoClampStyle,
    wordBreak: 'break-all',
    maxHeight: rowHeight === 'auto' && Number.isFinite(Number(maxContentHeightPx)) ? `${Number(maxContentHeightPx)}px` : '100%',
    height: rowHeight === 'auto' ? undefined : '100%',
    overflow: 'hidden',
    paddingRight: '4px',
    isolation: 'isolate',
    contain: 'layout style paint'
  }} data-html-content-scope="true" />;
}
export default HtmlContent;
