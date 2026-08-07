import { useState, useEffect, useRef, useMemo, useCallback, useLayoutEffect, forwardRef, useImperativeHandle } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { platform, version as osVersion } from '@tauri-apps/plugin-os';
import { toast, TOAST_POSITIONS, TOAST_SIZES } from '@shared/store/toastStore';
import { navigationStore } from '@shared/store/navigationStore';
import { Virtuoso } from 'react-virtuoso';
import { useCustomScrollbar } from '@shared/hooks/useCustomScrollbar';
import { useSnapshot } from 'valtio';
import { settingsStore } from '@shared/store/settingsStore';
import { restoreLastFocus } from '@shared/api/window';
import { ImageLibraryTab } from './emoji';
import ImageGroupModal from './emoji/ImageGroupModal';
import * as imageLibrary from '@shared/api/imageLibrary';
import Tooltip from '@shared/components/common/Tooltip.jsx';
import {
  SYMBOL_CATS, EMOJI_CATS, SKIN_TONES,
  RECENT_KEY, SKIN_TONE_KEY, MAX_RECENT,
  symbolCategories,
  ensureEmojiData, getEmojiDataCache, getEmojiMetaCache, getEmojiSkinSupport
} from './emoji/emojiData';
import {
  resolveSidebarCategoryId,
  resolveZoneNav,
  isSidebarCategoryActive,
} from './emoji/emojiKbNavigation';

const DEFAULT_GRID_COLS = 8;
const GRID_MIN_COLS = 4;
const GRID_MAX_COLS = 12;
const GRID_MIN_CELL_WIDTH = 42;
const GRID_GAP_PX = 2;
const GRID_HORIZONTAL_PADDING_PX = 8;
const EMOJI_TOAST_CONFIG = {
  size: TOAST_SIZES.EXTRA_SMALL,
  position: TOAST_POSITIONS.BOTTOM_RIGHT
};
const DEFAULT_IMAGE_GROUP_NAME = '默认';
const EMOJI_FALLBACK_FONT_TARGET = {
  windows: 'win10',
  linux: false,
  macos: false,
};
const EMOJI_FALLBACK_FONT_LINK_ID = 'qc-emoji-fallback-font';
const EMOJI_FALLBACK_FONT_STYLE_ID = 'qc-emoji-fallback-font-style';
const EMOJI_FALLBACK_FONT_URL = 'https://fonts.googleapis.com/css2?family=Noto+Color+Emoji';
const EMOJI_FALLBACK_FONT_FAMILY = '"Noto Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", sans-serif';
const EMOJI_FALLBACK_FONT_CLASS = 'qc-emoji-fallback-font';
let emojiFallbackFontLoadPromise = null;

const getWindowsBuildNumber = (rawVersion) => {
  const parts = String(rawVersion || '').match(/\d+/g);
  if (!parts || parts.length < 3) return 0;
  return Number(parts[2]) || 0;
};

const getWindowsFamily = () => {
  const buildNumber = getWindowsBuildNumber(osVersion());
  if (buildNumber >= 22000) return 'win11';
  if (buildNumber > 0) return 'win10';
  return 'unknown';
};

const shouldUseEmojiFallbackFont = () => {
  const currentPlatform = platform();
  const target = EMOJI_FALLBACK_FONT_TARGET[currentPlatform];
  if (!target) return false;
  if (target === true || target === 'all') return true;
  if (currentPlatform !== 'windows') return Boolean(target);
  return target === getWindowsFamily();
};

const ensureEmojiFallbackFontStyle = () => {
  if (document.getElementById(EMOJI_FALLBACK_FONT_STYLE_ID)) return;

  const style = document.createElement('style');
  style.id = EMOJI_FALLBACK_FONT_STYLE_ID;
  style.textContent = `
    .${EMOJI_FALLBACK_FONT_CLASS} {
      font-family: ${EMOJI_FALLBACK_FONT_FAMILY} !important;
      font-synthesis: none;
      font-variant-emoji: emoji;
    }
  `;
  document.head.appendChild(style);
};

const ensureEmojiFallbackFontLoaded = () => {
  if (emojiFallbackFontLoadPromise) return emojiFallbackFontLoadPromise;
  ensureEmojiFallbackFontStyle();

  emojiFallbackFontLoadPromise = new Promise((resolve, reject) => {
    const existingLink = document.getElementById(EMOJI_FALLBACK_FONT_LINK_ID);
    if (existingLink?.dataset.loaded === 'true') {
      resolve();
      return;
    }

    const link = existingLink || document.createElement('link');
    link.id = EMOJI_FALLBACK_FONT_LINK_ID;
    link.rel = 'stylesheet';
    link.href = EMOJI_FALLBACK_FONT_URL;

    link.onload = async () => {
      link.dataset.loaded = 'true';
      try {
        if (document.fonts?.load) {
          await document.fonts.load(`32px ${EMOJI_FALLBACK_FONT_FAMILY}`, '😀🫠🫨🪿');
        }
        resolve();
      } catch (e) {
        reject(e);
      }
    };
    link.onerror = () => reject(new Error('加载 Noto Color Emoji 字体失败'));

    if (!existingLink) {
      document.head.appendChild(link);
    }
  });

  return emojiFallbackFontLoadPromise;
};

const splitIntoRowsResponsive = (items, cols, catId) => {
  const rows = [];
  if (!Array.isArray(items) || items.length === 0) return rows;

  for (let i = 0; i < items.length; i += cols) {
    rows.push({
      type: 'row',
      items: items.slice(i, i + cols),
      cols,
      catId,
      id: `${catId}-row-${i}`
    });
  }

  return rows;
};

const getResponsiveGridCols = (width) => {
  if (!width) return DEFAULT_GRID_COLS;

  const availableWidth = Math.max(0, width - GRID_HORIZONTAL_PADDING_PX);
  const rawCols = Math.floor((availableWidth + GRID_GAP_PX) / (GRID_MIN_CELL_WIDTH + GRID_GAP_PX));
  return Math.max(GRID_MIN_COLS, Math.min(GRID_MAX_COLS, rawCols || DEFAULT_GRID_COLS));
};

const PreviewTooltipCard = ({ char, title, subtitle, codeLabel, sizeClass = 'text-[36px]', glyphClassName }) => {
  return (
    <div className="flex items-center gap-3 px-1 py-0.5">
      <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-qc-active ring-1 ring-qc-border">
        <span className={`${sizeClass} leading-none ${glyphClassName || ''}`}>{char}</span>
      </div>
      <div className="min-w-0">
        <div className="max-w-[220px] text-[15px] font-semibold leading-snug text-qc-fg break-words">
          {title}
        </div>
        {(subtitle || codeLabel) && (
          <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] leading-snug text-qc-fg-subtle">
            {subtitle ? <span>{subtitle}</span> : null}
            {codeLabel ? <span>{codeLabel}</span> : null}
          </div>
        )}
      </div>
    </div>
  );
};

const ImageGroupSidebarButton = forwardRef(function ImageGroupSidebarButton({
  group,
  isActive,
  onSelect,
  onEdit,
  isDropOver,
  t,
  buttonRef
}, ref) {
  return (
    <div
      ref={ref}
      data-image-group-name={group.name}
      className="group relative w-8 h-8 mx-auto mb-0.5"
    >
      <Tooltip content={group.name} placement="right" asChild>
        <button
          type="button"
          onClick={() => onSelect(group.name)}
          className={`w-full h-full flex items-center justify-center rounded-lg transition-colors ${
            isActive
              ? 'ring-2 ring-blue-500 ring-inset text-blue-600'
              : isDropOver
                ? 'bg-qc-active text-qc-fg'
                : 'text-qc-fg-muted hover:bg-qc-hover'
          }`}
        >
          <i
            className={`${group.icon || 'ti ti-photo'} text-base`}
            style={{ color: isActive ? undefined : (group.color || '#2563eb') }}
          ></i>
          {isDropOver && !isActive && (
            <span className="absolute inset-0 rounded-lg ring-2 ring-blue-400 pointer-events-none" />
          )}
        </button>
      </Tooltip>
      <button
        type="button"
        onClick={(e) => onEdit(e, group)}
        className="absolute -right-0.5 -top-0.5 w-3.5 h-3.5 rounded-full bg-qc-panel border border-qc-border text-qc-fg-muted hover:text-blue-600 hover:border-blue-400 opacity-0 hover:opacity-100 group-hover:opacity-100 flex items-center justify-center"
        title={t('groups.edit')}
      >
        <i className="ti ti-pencil text-[9px]"></i>
      </button>
    </div>
  );
});

function getImageGroupNameFromDragEvent(event) {
  const target = event.target?.closest?.('[data-image-group-name]');
  return target?.dataset?.imageGroupName || '';
}

const EmojiTab = forwardRef(function EmojiTab({ emojiMode, onEmojiModeChange, onSwitchTab, searchQuery = '' }, ref) {
  const showSymbols = emojiMode === 'symbols';
  const showImages = emojiMode === 'images';
  const { t } = useTranslation();
  const settings = useSnapshot(settingsStore);
  const isChinese = settings.language?.startsWith('zh');
  const [recentEmojis, setRecentEmojis] = useState([]);
  const [imageGroups, setImageGroups] = useState([]);
  const [imageGroupLoading, setImageGroupLoading] = useState(false);
  const [currentImageGroup, setCurrentImageGroup] = useState('');
  const [showImageGroupModal, setShowImageGroupModal] = useState(false);
  const [editingImageGroup, setEditingImageGroup] = useState(null);
  const [imageLibraryReloadKey, setImageLibraryReloadKey] = useState(0);
  const [imageDragOverGroup, setImageDragOverGroup] = useState('');
  const [skinTone, setSkinTone] = useState(() => localStorage.getItem(SKIN_TONE_KEY) || 'default');
  const [skinPickerEmoji, setSkinPickerEmoji] = useState(null);
  const [isReady, setIsReady] = useState(false);
  const [isModeReady, setIsModeReady] = useState(true);
  const [contentWidth, setContentWidth] = useState(0);
  const [useEmojiFallbackFont, setUseEmojiFallbackFont] = useState(false);
  // 键盘区域导航:默认 outside(无高亮,←/→ 切主标签);↓ 经 App 转发激活
  // 主路径是后端 global hotkey → App.dispatchEmojiNav → handleNavAction(不依赖 webview keydown)
  const [kbZone, setKbZone] = useState('outside'); // 'outside' | 'search' | 'grid' | 'sidebar' | 'tabbar'
  const [kbRow, setKbRow] = useState(-1);
  const [kbCol, setKbCol] = useState(0);
  const kbRowRef = useRef(kbRow);
  const kbColRef = useRef(kbCol);
  const kbZoneRef = useRef(kbZone);
  kbRowRef.current = kbRow;
  kbColRef.current = kbCol;
  kbZoneRef.current = kbZone;
  const imageLibraryRef = useRef(null);
  const prevEmojiModeRef = useRef(emojiMode);
  const activeImageDragItemsRef = useRef([]);
  const imagePluginDragClearTimerRef = useRef(null);
  const scrollContainerRef = useRef(null);
  const contentMeasureRef = useRef(null);
  const activeCategoryRef = useRef('recent');
  // 侧栏高亮重渲信号:ref 是唯一真值,改 ref 后 bump 此 tick 触发渲染读新值
  const [sidebarHighlightTick, setSidebarHighlightTick] = useState(0);
  const sidebarButtonsRef = useRef({});
  const virtualDataRef = useRef([]);
  const emojiMetaRef = useRef({});
  const isUserScrollingRef = useRef(false);
  const [scrollerElement, setScrollerElement] = useState(null);
  const scrollerRefCallback = useCallback(element => element && setScrollerElement(element), []);
  useCustomScrollbar(scrollerElement);
  const gridCols = useMemo(() => getResponsiveGridCols(contentWidth), [contentWidth]);
  const emojiGlyphClassName = useEmojiFallbackFont ? EMOJI_FALLBACK_FONT_CLASS : '';

  useEffect(() => {
    let cancelled = false;

    try {
      if (!shouldUseEmojiFallbackFont()) return undefined;
      ensureEmojiFallbackFontLoaded()
        .then(() => {
          if (!cancelled) setUseEmojiFallbackFont(true);
        })
        .catch(e => {
          console.warn('加载 Noto Color Emoji 字体失败:', e);
        });
    } catch (e) {
      console.warn('判断 Windows 版本失败:', e);
    }

    return () => {
      cancelled = true;
    };
  }, []);

  const loadImageGroups = useCallback(async (preferredGroup = '') => {
    setImageGroupLoading(true);
    try {
      const groups = await imageLibrary.getImageGroups();
      if (!Array.isArray(groups)) {
        setImageGroups([]);
        setCurrentImageGroup('');
        return [];
      }

      setImageGroups(groups);
      setCurrentImageGroup(prev => {
        const preferred = preferredGroup && groups.some(group => group.name === preferredGroup)
          ? preferredGroup
          : '';
        if (preferred) return preferred;
        if (prev && groups.some(group => group.name === prev)) return prev;
        return groups[0]?.name || '';
      });
      return groups;
    } catch (error) {
      console.error('加载图库分组失败:', error);
      setImageGroups([]);
      setCurrentImageGroup('');
      return [];
    } finally {
      setImageGroupLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!showImages) return undefined;
    loadImageGroups();
    return undefined;
  }, [showImages, loadImageGroups]);

  useEffect(() => {
    const timer = setTimeout(() => {
      ensureEmojiData();
      emojiMetaRef.current = getEmojiMetaCache() || {};
      setIsReady(true);
    }, 300);
    return () => {
      clearTimeout(timer);
      if (imagePluginDragClearTimerRef.current) {
        window.clearTimeout(imagePluginDragClearTimerRef.current);
      }
    };
  }, []);

  useLayoutEffect(() => {
    if (showImages) return undefined;

    const target = contentMeasureRef.current;
    if (!target) return undefined;

    let rafId = 0;
    let observer = null;

    const measure = () => {
      rafId = requestAnimationFrame(() => {
        const nextWidth = target.clientWidth || 0;
        setContentWidth(prev => (prev === nextWidth ? prev : nextWidth));
      });
    };

    measure();

    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(measure);
      observer.observe(target);
    } else {
      window.addEventListener('resize', measure);
    }

    return () => {
      if (rafId) {
        cancelAnimationFrame(rafId);
      }
      if (observer) {
        observer.disconnect();
      } else {
        window.removeEventListener('resize', measure);
      }
    };
  }, [showImages]);


  // 加载最近使用
  useEffect(() => {
    try {
      const stored = localStorage.getItem(RECENT_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          const normalized = parsed.map(item =>
            typeof item === 'string'
              ? { value: item, name: item, nameCn: item }
              : {
                  value: item?.value || item?.emoji || '',
                  name: item?.name || item?.title || item?.value || '',
                  nameCn: item?.nameCn || item?.name || item?.title || item?.value || ''
                }
          ).filter(entry => entry.value);
          setRecentEmojis(normalized);
        }
      }
    } catch (e) {}
  }, []);


  const formatSymbolTitle = useCallback((char, catId) => {
    const cat = SYMBOL_CATS.find(c => c.id === catId);
    const label = cat ? t(cat.labelKey) : t('emoji.symbols');
    const cp = char?.codePointAt?.(0);
    return cp ? `${label} · U+${cp.toString(16).toUpperCase().padStart(4, '0')}` : label;
  }, [t]);

  // 保存最近使用
  const addToRecent = useCallback((value, name, nameCn) => {
    const meta = emojiMetaRef.current[value];
    const entry = {
      value,
      name: name || meta?.name || value,
      nameCn: nameCn || meta?.nameCn || meta?.name || value
    };
    setRecentEmojis(prev => {
      const filtered = prev.filter(item => item.value !== value);
      const updated = [entry, ...filtered].slice(0, MAX_RECENT);
      localStorage.setItem(RECENT_KEY, JSON.stringify(updated));
      return updated;
    });
  }, []);

  // 获取 emoji 的肤色变体
  const getSkinVariants = useCallback((emoji) => {
    const skins = getEmojiSkinSupport()?.get(emoji);
    if (!skins) return null;
    return [emoji, ...skins];
  }, []);

  const applySkintone = useCallback((emoji) => {
    if (skinTone === 'default') return emoji;
    const skins = getEmojiSkinSupport()?.get(emoji);
    if (!skins) return emoji;
    
    const toneIndex = SKIN_TONES.findIndex(t => t.id === skinTone);
    if (toneIndex <= 0) return emoji;
    return skins[toneIndex - 1] || emoji;
  }, [skinTone]);

  const updateSkinToneFromEmoji = useCallback((emoji, baseEmoji) => {
    const skins = getEmojiSkinSupport()?.get(baseEmoji);
    if (!skins) return;
    
    if (emoji === baseEmoji) {
      setSkinTone('default');
      localStorage.setItem(SKIN_TONE_KEY, 'default');
    } else {
      const idx = skins.indexOf(emoji);
      if (idx >= 0 && SKIN_TONES[idx + 1]) {
        setSkinTone(SKIN_TONES[idx + 1].id);
        localStorage.setItem(SKIN_TONE_KEY, SKIN_TONES[idx + 1].id);
      }
    }
  }, []);

  const handleSkinPickerOpen = useCallback((e, baseChar, variants, item, catId) => {
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    setSkinPickerEmoji({ baseChar, variants, rect, item, catId });
  }, []);

  // 粘贴
  const handlePaste = useCallback(async (item, catId, skinVariant, baseEmoji) => {
    let char = skinVariant || (typeof item === 'string' ? item : item?.emoji);
    if (!char) return;
    
    if (skinVariant && baseEmoji) {
      updateSkinToneFromEmoji(skinVariant, baseEmoji);
    }
    else if ((catId === 'people-body' || catId === 'recent') && !skinVariant) {
      char = applySkintone(char);
    }
    const meta = emojiMetaRef.current[char];
    const name = typeof item === 'object' ? (item.name || meta?.name) : meta?.name;
    const nameCn = typeof item === 'object' ? (item.nameCn || meta?.nameCn) : meta?.nameCn;
    try {
      await restoreLastFocus();
      await invoke('paste_text_direct', { text: char });
      addToRecent(char, name, nameCn);
    } catch (e) {
      console.error('粘贴失败:', e);
      toast.error(t('common.error'), EMOJI_TOAST_CONFIG);
    }
  }, [addToRecent, t, applySkintone, updateSkinToneFromEmoji]);

  // 搜索结果
  const searchResults = useMemo(() => {
    const rawQuery = searchQuery.trim();
    if (!rawQuery) return null;
    const query = rawQuery.toLowerCase();
    
    const emojiResults = [];
    const emojiDataCache = getEmojiDataCache();
    if (emojiDataCache) Object.values(emojiDataCache).forEach(emojis => {
      emojis.forEach(item => {
        if (item.name?.toLowerCase().includes(query) || item.nameCn?.includes(rawQuery)) {
          emojiResults.push(item);
        }
      });
    });
    
    const symbolResults = [];
    SYMBOL_CATS.forEach(cat => {
      (symbolCategories[cat.id] || []).forEach(ch => {
        if (ch.includes(rawQuery) || ch.toLowerCase().includes(query)) {
          symbolResults.push({ emoji: ch, name: formatSymbolTitle(ch, cat.id) });
        }
      });
    });
    
    return { emojis: emojiResults.slice(0, 100), symbols: symbolResults.slice(0, 50) };
  }, [searchQuery, formatSymbolTitle]);

  const symbolRowsCache = useMemo(() => {
    const cache = {};
    SYMBOL_CATS.forEach(cat => {
      const symbols = (symbolCategories[cat.id] || []).map(ch => ({
        emoji: ch,
        name: formatSymbolTitle(ch, cat.id)
      }));
      cache[cat.id] = splitIntoRowsResponsive(symbols, gridCols, cat.id);
    });
    return cache;
  }, [formatSymbolTitle, gridCols]);

  // 构建虚拟列表数据
  const virtualData = useMemo(() => {
    if (!isModeReady) return [];
    const emojiDataCache = getEmojiDataCache();
    
    if (searchQuery && searchResults) {
      const sections = [];
      if (searchResults.emojis.length > 0) {
        sections.push({ type: 'header', title: t('emoji.searchResults'), id: 'header-search-emoji' });
        sections.push(...splitIntoRowsResponsive(searchResults.emojis, gridCols, 'search-emoji'));
      }
      if (searchResults.symbols.length > 0) {
        sections.push({ type: 'header', title: t('emoji.symbolResults'), id: 'header-search-symbol' });
        sections.push(...splitIntoRowsResponsive(searchResults.symbols, gridCols, 'search-symbol'));
      }
      if (sections.length === 0) {
        sections.push({ type: 'empty', id: 'no-results' });
      }
      return sections;
    }
    
    if (showSymbols) {
      const sections = [];
      SYMBOL_CATS.forEach(cat => {
        const rows = symbolRowsCache[cat.id];
        if (rows?.length > 0) {
          sections.push({ type: 'header', title: t(cat.labelKey), id: `header-${cat.id}` });
          sections.push(...rows);
        }
      });
      return sections;
    }
    
    if (!emojiDataCache) return [];
    const sections = [];
    // 最近使用
    sections.push({ type: 'header', title: t('emoji.recent'), id: 'header-recent' });
    if (recentEmojis.length > 0) {
      const recentEntries = recentEmojis.map(item => ({
        emoji: item.value,
        name: item.name,
        nameCn: item.nameCn
      }));
      sections.push(...splitIntoRowsResponsive(recentEntries, gridCols, 'recent'));
    } else {
      sections.push({ type: 'empty-recent', id: 'empty-recent' });
    }
    EMOJI_CATS.filter(c => c.id !== 'recent').forEach(cat => {
      const rows = splitIntoRowsResponsive(emojiDataCache?.[cat.id] || [], gridCols, cat.id);
      if (rows?.length > 0) {
        sections.push({ type: 'header', title: t(cat.labelKey), id: `header-${cat.id}` });
        sections.push(...rows);
      }
    });
    return sections;
  }, [searchQuery, searchResults, emojiMode, recentEmojis, t, symbolRowsCache, isReady, isModeReady, gridCols]);

  virtualDataRef.current = virtualData;

  // 侧栏高亮受控:只改 ref 真值 + 强制重渲,禁止 classList 手改(re-render 会盖掉)
  // 单一真值:activeCategoryRef(enterSidebar/moveSidebarBy/高亮渲染都读它)
  const updateSidebarHighlight = useCallback((catId) => {
    if (activeCategoryRef.current === catId) return;
    activeCategoryRef.current = catId;
    setSidebarHighlightTick(tick => tick + 1);
  }, []);

  useEffect(() => {
    // 切换子模式时重置键盘导航状态:回到 outside(无高亮,←/→ 恢复原功能)
    // emojiKbActive 由下方 useEffect([kbZone]) 单点兜底,这里只改 zone
    setKbZone('outside');
    setKbRow(-1);
    setKbCol(0);
    imageLibraryRef.current?.resetKbIndex?.();

    if (showImages) {
      loadImageGroups(currentImageGroup);
    } else {
      const firstCat = showSymbols ? SYMBOL_CATS[0]?.id : EMOJI_CATS[0]?.id;
      if (firstCat) {
        activeCategoryRef.current = firstCat;
        setSidebarHighlightTick(tick => tick + 1);
        scrollContainerRef.current?.scrollToIndex({ index: 0 });
      }
    }
  }, [emojiMode]);

  useEffect(() => {
    const prevMode = prevEmojiModeRef.current;
    prevEmojiModeRef.current = emojiMode;
    
    if (emojiMode === 'emoji' && prevMode !== 'emoji') {
      setIsModeReady(false);
      const timer = setTimeout(() => {
        setIsModeReady(true);
      }, 250);
      return () => clearTimeout(timer);
    }
  }, [emojiMode]);

  const scrollToCategory = useCallback((categoryId) => {
    isUserScrollingRef.current = true;
    updateSidebarHighlight(categoryId);
    const targetId = `header-${categoryId}`;
    const index = virtualData.findIndex(item => item.id === targetId);
    if (index >= 0) {
      scrollContainerRef.current?.scrollToIndex({ index, align: 'start' });
    } else {
      scrollContainerRef.current?.scrollToIndex({ index: 0 });
    }
    setTimeout(() => {
      isUserScrollingRef.current = false;
    }, 100);
  }, [virtualData, updateSidebarHighlight]);

  const handleRangeChanged = useCallback((range) => {
    if (isUserScrollingRef.current) return;
    
    const data = virtualDataRef.current;
    const item = data[range.startIndex];
    if (!item) return;
    
    let foundCatId = null;
    if (item.type === 'header') {
      foundCatId = item.id.replace('header-', '');
    } else if (item.type === 'row') {
      foundCatId = item.catId;
    }
    
    if (foundCatId) {
      updateSidebarHighlight(foundCatId);
    }
  }, [updateSidebarHighlight]);

  const renderVirtualItem = useCallback((index) => {
    const section = virtualDataRef.current[index];
    if (!section) return null;
    const uiAnimationEnabled = settingsStore.uiAnimationEnabled !== false;
    
    if (section.type === 'header') {
      return (
        <div className="sticky top-0 z-10 px-2 py-1.5 text-xs font-medium text-qc-fg-muted bg-qc-panel/90 backdrop-blur-sm">
          {section.title}
        </div>
      );
    }
    
    if (section.type === 'empty') {
      return <div className="text-center text-qc-fg-subtle py-8 text-sm">{t('emoji.noResults')}</div>;
    }
    
    if (section.type === 'empty-recent') {
      return <div className="px-2 py-3 text-xs text-qc-fg-subtle text-center">{t('emoji.noRecent')}</div>;
    }
    
    if (section.type === 'row') {
      const shouldApplySkin = section.catId === 'people-body';
      return (
        <div 
          className="grid gap-0.5 px-1" 
          style={{ 
            gridTemplateColumns: `repeat(${section.cols}, minmax(0, 1fr))`,
            contentVisibility: 'auto',
            containIntrinsicSize: '0 36px'
          }}
        >
          {section.items.map((item, idx) => {
            const baseChar = typeof item === 'string' ? item : item.emoji;
            const displayChar = shouldApplySkin ? applySkintone(baseChar) : baseChar;
            const skinVariants = shouldApplySkin ? getSkinVariants(baseChar) : null;
            const meta = emojiMetaRef.current[baseChar];
            const codePoint = baseChar?.codePointAt?.(0);
            const codeLabel = codePoint ? `U+${codePoint.toString(16).toUpperCase().padStart(4, '0')}` : '';
            const name = typeof item === 'object'
              ? (isChinese ? (item.nameCn || item.name) : (item.name || item.nameCn))
              : (isChinese ? (meta?.nameCn || meta?.name) : (meta?.name || meta?.nameCn)) || baseChar;
            return (
              <div key={`${baseChar}-${idx}`} className="relative group">
                <Tooltip
                  content={
                    <PreviewTooltipCard
                      char={displayChar}
                      title={name}
                      subtitle={section.catId === 'symbols'
                        ? t('emoji.symbols')
                        : (section.catId === 'people-body' ? t('emoji.people') : '')}
                      codeLabel={codeLabel}
                      sizeClass={section.catId === 'symbols' ? 'text-[27px]' : 'text-[34px]'}
                      glyphClassName={emojiGlyphClassName}
                    />
                  }
                  placement="top"
                  maxWidth={360}
                  asChild
                >
                  <button
                    onClick={() => handlePaste(item, section.catId)}
                    className={`aspect-square w-full flex items-center justify-center text-2xl leading-none text-qc-fg rounded cursor-pointer transition-[transform,box-shadow,background-color,border-color] ${uiAnimationEnabled ? 'active:scale-95 hover:bg-qc-panel hover:shadow-lg hover:border hover:border-qc-border' : 'hover:bg-qc-hover'} ${
                      kbZone === 'grid' && index === kbRow && idx === kbCol ? 'ring-2 ring-blue-500 ring-inset' : ''
                    }`}
                    style={uiAnimationEnabled ? {
                      opacity: 0,
                      animation: `fadeIn 0.15s ease-out ${idx * 15}ms forwards`
                    } : {}}
                  >
                    <span
                      className={`inline-flex items-center justify-center w-[1.2em] h-[1.2em] overflow-hidden ${emojiGlyphClassName}`}
                    >
                      {displayChar}
                    </span>
                  </button>
                </Tooltip>
                {/* 肤色选择按钮 */}
                {skinVariants && (
                  <Tooltip content="选择肤色" placement="left" asChild>
                    <button
                      onClick={(e) => handleSkinPickerOpen(e, baseChar, skinVariants, item, section.catId)}
                      className={`absolute top-0.5 right-0.5 z-10 w-3 h-3 rounded-full bg-gradient-to-br from-amber-200 via-amber-400 to-amber-700 border border-white opacity-0 group-hover:opacity-100 shadow-sm ${uiAnimationEnabled ? 'transition-opacity hover:scale-125' : ''}`}
                    />
                  </Tooltip>
                )}
              </div>
            );
          })}
        </div>
      );
    }
    return null;
  }, [handlePaste, isChinese, skinTone, applySkintone, getSkinVariants, handleSkinPickerOpen, emojiGlyphClassName, kbZone, kbRow, kbCol]);

  const currentCategories = useMemo(() => {
    if (showImages) return imageGroups.map(group => ({
      id: group.name,
      icon: group.icon || 'ti ti-photo',
      label: group.name,
      color: group.color || '#2563eb',
      itemCount: group.item_count || 0
    }));
    if (showSymbols) return SYMBOL_CATS;
    return EMOJI_CATS;
  }, [showImages, showSymbols, imageGroups]);

  const handleImageGroupClick = useCallback((groupName) => {
    setCurrentImageGroup(groupName);
  }, []);

  const handleCategoryClick = useCallback((catId) => {
    if (showImages) {
      handleImageGroupClick(catId);
    } else {
      scrollToCategory(catId);
    }
  }, [showImages, handleImageGroupClick, scrollToCategory]);

  const handleAddImageGroup = useCallback(() => {
    setEditingImageGroup(null);
    setShowImageGroupModal(true);
  }, []);

  const handleEditImageGroup = useCallback((e, group) => {
    e.stopPropagation();
    setEditingImageGroup(group);
    setShowImageGroupModal(true);
  }, []);

  const handleImageGroupSaved = useCallback(async (savedGroup) => {
    setShowImageGroupModal(false);
    setEditingImageGroup(null);
    if (savedGroup?.deleted) {
      const groups = Array.isArray(savedGroup.groups)
        ? savedGroup.groups
        : await loadImageGroups(DEFAULT_IMAGE_GROUP_NAME);
      setImageGroups(groups);
      const nextName = groups.find(group => group.name === DEFAULT_IMAGE_GROUP_NAME)?.name
        || groups[0]?.name
        || '';
      setCurrentImageGroup(nextName);
      setImageLibraryReloadKey(prev => prev + 1);
      toast.success(t('common.deleted'), EMOJI_TOAST_CONFIG);
      return;
    }

    const nextName = savedGroup?.name || currentImageGroup;
    await loadImageGroups(nextName);
  }, [currentImageGroup, loadImageGroups, t]);

  const refreshImageLibraryGroups = useCallback(async (preferredGroup = currentImageGroup) => {
    const groups = await loadImageGroups(preferredGroup);
    return groups;
  }, [currentImageGroup, loadImageGroups]);

  const clearImagePluginDragState = useCallback(() => {
    if (imagePluginDragClearTimerRef.current) {
      window.clearTimeout(imagePluginDragClearTimerRef.current);
      imagePluginDragClearTimerRef.current = null;
    }
    activeImageDragItemsRef.current = [];
    setImageDragOverGroup('');
  }, []);

  const handleImagePluginDragStart = useCallback((items) => {
    if (imagePluginDragClearTimerRef.current) {
      window.clearTimeout(imagePluginDragClearTimerRef.current);
      imagePluginDragClearTimerRef.current = null;
    }
    const dragItems = Array.isArray(items) ? items : [items];
    activeImageDragItemsRef.current = dragItems.filter(item => item?.path && item?.filename);
    setImageDragOverGroup('');
  }, []);

  const handleImagePluginDragEnd = useCallback(() => {
    if (imagePluginDragClearTimerRef.current) {
      window.clearTimeout(imagePluginDragClearTimerRef.current);
    }
    imagePluginDragClearTimerRef.current = window.setTimeout(() => {
      clearImagePluginDragState();
    }, 250);
  }, [clearImagePluginDragState]);

  // ==== 键盘区域导航 ====
  // 主路径:后端 hotkey → App.dispatchEmojiNav → handleNavAction
  // zone: outside(默认) → search(↓激活) ↔ grid/sidebar; search↑ → outside
  const enterGrid = useCallback(() => {
    if (showImages) {
      const ok = imageLibraryRef.current?.activateKb?.();
      if (ok) {
        setKbZone('grid');
        return;
      }
      // G6 修:图片库异步未就绪(activateKb 返回 false)时降级到搜索框,
      // 保留键盘导航态(search)给用户视觉反馈,而不是静默吞掉 ↓ 键
      focusSearchInput();
      return;
    }
    const data = virtualDataRef.current;
    const firstRowIndex = data.findIndex(section => section.type === 'row');
    if (firstRowIndex === -1) return;
    setKbRow(firstRowIndex);
    setKbCol(0);
    setKbZone('grid');
    scrollContainerRef.current?.scrollToIndex({ index: firstRowIndex, align: 'center' });
  }, [showImages]);

  const enterSidebar = useCallback(() => {
    const cats = currentCategories;
    // 图库侧栏以 currentImageGroup 为 active;emoji/符号用 activeCategoryRef
    const activeId = showImages ? currentImageGroup : activeCategoryRef.current;
    const catId = resolveSidebarCategoryId(cats, activeId);
    if (!catId) return;
    setKbZone('sidebar');
    // 保留当前分类,不强制 cats[0]
    handleCategoryClick(catId);
    // 聚焦当前分类按钮(键盘焦点跟随区域)
    const btn = sidebarButtonsRef.current[catId];
    btn?.focus?.();
  }, [currentCategories, handleCategoryClick, showImages, currentImageGroup]);

  const focusSearchInput = useCallback(() => {
    setKbZone('search');
  }, []);

  const blurSearchInput = useCallback(() => {
    setKbZone('outside');
    setKbRow(-1);
    setKbCol(0);
    imageLibraryRef.current?.resetKbIndex?.();
  }, []);

  // G3:过滤热键路径(App handleFilterLeft/Right)切子模式前调用——把 kbZone 置
  // outside,让 emojiMode effect 的 setKbZone('outside') 同值短路,effect 不跑,
  // 键盘导航态(如 grid 高亮)得以保留
  const resetKbNav = useCallback(() => {
    setKbZone('outside');
    setKbRow(-1);
    setKbCol(0);
    imageLibraryRef.current?.resetKbIndex?.();
  }, []);
  const getKbZone = useCallback(() => kbZoneRef.current, []);

  const moveSidebarBy = useCallback((delta) => {
    const cats = currentCategories;
    if (cats.length === 0) return false;
    // 图库用 currentImageGroup;emoji/符号用 activeCategoryRef
    const currentId = showImages ? currentImageGroup : activeCategoryRef.current;
    let idx = cats.findIndex(c => c.id === currentId);
    if (idx < 0) idx = 0;
    const target = idx + delta;
    if (target < 0 || target >= cats.length) return false;
    handleCategoryClick(cats[target].id);
    // 聚焦新分类按钮
    const btn = sidebarButtonsRef.current[cats[target].id];
    btn?.focus?.();
    return true;
  }, [currentCategories, handleCategoryClick, showImages, currentImageGroup]);

  const moveGridBy = useCallback((dRow, dCol) => {
    if (showImages) {
      const api = imageLibraryRef.current;
      if (!api) return false;
      if (dRow === -1) return api.navigateUp();
      if (dRow === 1) return api.navigateDown();
      if (dCol === -1) return api.navigateLeft();
      if (dCol === 1) return api.navigateRight();
      return false;
    }
    const data = virtualDataRef.current;
    const rowIndexes = [];
    data.forEach((section, index) => {
      if (section.type === 'row') rowIndexes.push(index);
    });
    if (rowIndexes.length === 0) return false;

    let currentPos = rowIndexes.indexOf(kbRowRef.current);
    let targetRowPos;
    let targetCol = kbColRef.current;

    if (currentPos === -1) {
      targetRowPos = dRow >= 0 ? 0 : rowIndexes.length - 1;
      targetCol = 0;
    } else {
      targetRowPos = currentPos + dRow;
      if (dCol !== 0) targetCol = kbColRef.current + dCol;
    }

    // 行内左右移动:列夹到当前行边界,越界返回 false(让 caller 切 zone)
    if (dCol !== 0) {
      if (currentPos === -1) return false;
      const curRow = data[rowIndexes[currentPos]];
      if (targetCol < 0 || targetCol >= curRow.items.length) return false;
    }

    if (targetRowPos < 0 || targetRowPos >= rowIndexes.length) return false;

    const rowIndex = rowIndexes[targetRowPos];
    const row = data[rowIndex];
    const clampedCol = Math.max(0, Math.min(targetCol, row.items.length - 1));
    setKbRow(rowIndex);
    setKbCol(clampedCol);
    scrollContainerRef.current?.scrollToIndex({ index: rowIndex, align: 'center' });
    return true;
  }, [showImages]);

  const gridHome = useCallback(() => {
    if (showImages) {
      // 图片网格列数与表情不同,行首计算在 ImageLibraryTab 内(用自身 imageCols)
      imageLibraryRef.current?.goHome?.();
      return;
    }
    const data = virtualDataRef.current;
    const rowIndexes = [];
    data.forEach((section, index) => {
      if (section.type === 'row') rowIndexes.push(index);
    });
    if (rowIndexes.length === 0) return;
    const firstRow = rowIndexes[0];
    setKbRow(firstRow);
    setKbCol(0);
    scrollContainerRef.current?.scrollToIndex({ index: firstRow, align: 'center' });
  }, [showImages, gridCols]);

  // 执行 zone 意图(resolveZoneNav 的 type)
  const applyNavIntent = useCallback((intent) => {
    if (!intent || intent.type === 'none') return;
    switch (intent.type) {
      case 'activate-search':
      case 'enter-search':
        focusSearchInput();
        break;
      case 'enter-grid':
        enterGrid();
        break;
      case 'enter-sidebar':
        enterSidebar();
        break;
      case 'prev-mode':
        // 侧栏再 ← 切上一个子模式:图片→符号→表情;表情(最左)再 ← 切收藏主标签。
        // 子模式 effect 会重置到该模式起点(第一个表情),键盘导航态自然衔接。
        resetKbNav();
        if (emojiMode === 'emoji') {
          // 表情是最左子模式,再往左切收藏主标签
          setKbZone('outside');
          onSwitchTab?.('favorites');
        } else {
          onEmojiModeChange?.(cycleValue(['emoji', 'symbols', 'images'], emojiMode, -1));
        }
        break;
      case 'deactivate':
        blurSearchInput();
        break;
      case 'grid-move': {
        const ok = moveGridBy(intent.dRow, intent.dCol);
        if (!ok && intent.onFail === 'enter-search') focusSearchInput();
        if (!ok && intent.onFail === 'enter-sidebar') enterSidebar();
        // 最右列 → 越界回到当前分类第一个格子(不切子模式)
        if (!ok && intent.onFail === 'grid-home') {
          gridHome();
        }
        break;
      }
      case 'sidebar-move': {
        const ok = moveSidebarBy(intent.delta);
        if (!ok && intent.onFail === 'enter-search') focusSearchInput();
        break;
      }
      default:
        break;
    }
  }, [focusSearchInput, enterGrid, enterSidebar, blurSearchInput, moveGridBy, moveSidebarBy, gridHome, onSwitchTab]);

  // 主路径:App 转发后端 navigation-action。不挂 window keydown,
  // 避免与 RegisterHotKey 在搜索框聚焦时双触发(进 grid 两次/跳格)。
  const handleNavAction = useCallback((action) => {
    if (skinPickerEmoji || showImageGroupModal) return;
    const intent = resolveZoneNav(kbZoneRef.current, action);
    applyNavIntent(intent);
  }, [skinPickerEmoji, showImageGroupModal, applyNavIntent]);

  // 同步 emojiKbActive 到 navigationStore(兜底:任何 setKbZone 路径都覆盖)
  // G7 时序边界说明:store 写发生在 effect 提交期,晚于本帧 render 的
  // kbZoneRef 同步(render 期)。连续两次 ↓(间隔 <16ms 同一提交批次)时,
  // useNavigationKeyboard listen 读 store=true 但 kbZoneRef 仍是旧值,
  // resolveZoneNav 决策基于旧 zone。verifier 实证无用户可见 bug(<16ms
  // 自动连发才可达,人工按键间隔远超),此单点写是刻意设计:任何 setKbZone
  // 路径都覆盖,避免双写竞态(F8 已删 5 处显式写收敛于此)。
  useEffect(() => {
    navigationStore.setEmojiKbActive(kbZone !== 'outside');
  }, [kbZone]);

  // 虚拟列表数据变化时:kbRow 越界时夹住;网格为空且 zone='grid' 时回 outside
  useEffect(() => {
    const data = virtualDataRef.current;
    const rowIndexes = [];
    data.forEach((section, index) => {
      if (section.type === 'row') rowIndexes.push(index);
    });
    if (rowIndexes.length === 0) {
      if (kbZone === 'grid') {
        setKbZone('outside');
        setKbRow(-1);
        setKbCol(0);
        imageLibraryRef.current?.resetKbIndex?.();
      }
      return;
    }
    if (kbRowRef.current >= 0 && !rowIndexes.includes(kbRowRef.current)) {
      setKbRow(rowIndexes[0]);
      setKbCol(0);
    }
  }, [virtualData, kbZone]);

  // 输出选中项:图片模式转发图库,emoji/符号粘贴 kbRow/kbCol 格
  // tabbar 态 Enter 走 TabNavigation 选中(handleKbNav 切换),此处不得用
  // 网格陈旧坐标粘贴——search 态同理(无网格选中),直接 no-op。
  const executeCurrentItem = useCallback(() => {
    const zone = kbZoneRef.current;
    if (zone !== 'grid') return;
    if (showImages) {
      imageLibraryRef.current?.executeCurrent?.();
      return;
    }
    const data = virtualDataRef.current;
    const row = data[kbRowRef.current];
    if (!row || row.type !== 'row' || kbColRef.current >= row.items.length) return;
    const item = row.items[kbColRef.current];
    const catId = row.catId;
    handlePaste(item, catId);
  }, [showImages, handlePaste]);

  useImperativeHandle(ref, () => ({
    executeCurrentItem,
    handleNavAction,
    resetKbNav,
    getKbZone
  }), [executeCurrentItem, handleNavAction, resetKbNav, getKbZone]);

  const moveActiveImageToGroup = useCallback(async (targetGroup) => {
    const items = activeImageDragItemsRef.current || [];
    if (!items.length || !targetGroup) return;

    const movableItems = items.filter(item => {
      const sourceGroup = item.group || item.category;
      return sourceGroup && sourceGroup !== targetGroup && item.filename;
    });
    if (!movableItems.length) return;

    try {
      await Promise.all(movableItems.map(item => {
        const sourceGroup = item.group || item.category;
        return imageLibrary.moveImageToGroup(sourceGroup, item.filename, targetGroup);
      }));
      toast.success(t('emoji.movedToGroup', { group: targetGroup }) || `已移动到 ${targetGroup}`, EMOJI_TOAST_CONFIG);
      setImageLibraryReloadKey(prev => prev + 1);
      await loadImageGroups(currentImageGroup);
    } catch (error) {
      console.error('移动图片分组失败:', error);
      toast.error(t('emoji.moveToGroupFailed') || '移动分组失败', EMOJI_TOAST_CONFIG);
    }
  }, [currentImageGroup, loadImageGroups, t]);

  const handleImageSidebarDragEnter = useCallback((event) => {
    if (!showImages) return;
    event.preventDefault();
    event.stopPropagation();
    const groupName = getImageGroupNameFromDragEvent(event);
    if (!activeImageDragItemsRef.current.length) return;
    setImageDragOverGroup(groupName);
  }, [showImages]);

  const handleImageSidebarDragOver = useCallback((event) => {
    if (!showImages) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = 'copy';
    }
    const groupName = getImageGroupNameFromDragEvent(event);
    if (!activeImageDragItemsRef.current.length) return;
    setImageDragOverGroup(groupName);
  }, [showImages]);

  const handleImageSidebarDragLeave = useCallback((event) => {
    if (!showImages) return;
    if (event.currentTarget.contains(event.relatedTarget)) return;
    setImageDragOverGroup('');
  }, [showImages]);

  const handleImageSidebarDrop = useCallback(async (event) => {
    if (!showImages) return;
    event.preventDefault();
    event.stopPropagation();
    const groupName = getImageGroupNameFromDragEvent(event);
    setImageDragOverGroup('');
    await moveActiveImageToGroup(groupName);
    clearImagePluginDragState();
  }, [clearImagePluginDragState, moveActiveImageToGroup, showImages]);

  return (
    <div className="h-full flex bg-qc-surface">
      {/* 侧边分类栏 */}
      <div
        className="emoji-sidebar w-10 flex-shrink-0 bg-qc-panel border-r border-qc-border flex flex-col py-1 overflow-y-auto scrollbar-hide"
        onDragEnter={handleImageSidebarDragEnter}
        onDragOver={handleImageSidebarDragOver}
        onDragLeave={handleImageSidebarDragLeave}
        onDrop={handleImageSidebarDrop}
      >
        {/* 分类按钮 */}
        {showImages ? (
          <>
            {imageGroups.map(group => (
              <ImageGroupSidebarButton
                key={group.name}
                group={group}
                isActive={currentImageGroup === group.name}
                onSelect={handleImageGroupClick}
                onEdit={handleEditImageGroup}
                isDropOver={imageDragOverGroup === group.name}
                t={t}
                ref={el => { sidebarButtonsRef.current[group.name] = el; }}
              />
            ))}
            {imageGroupLoading && imageGroups.length === 0 && (
              <div className="w-8 h-8 mx-auto mb-0.5 flex items-center justify-center text-qc-fg-subtle">
                <i className="ti ti-loader-2 animate-spin text-base"></i>
              </div>
            )}
            <Tooltip content={t('groups.add')} placement="right" asChild>
              <button
                type="button"
                onClick={handleAddImageGroup}
                className="w-8 h-8 mx-auto mt-auto flex items-center justify-center rounded-lg transition-colors text-qc-fg-muted hover:bg-qc-hover hover:text-blue-600"
              >
                <i className="ti ti-plus text-base"></i>
              </button>
            </Tooltip>
          </>
        ) : currentCategories.map((cat, idx) => (
          <Tooltip key={cat.id} content={t(cat.labelKey)} placement="right" asChild>
            <button
              ref={el => { sidebarButtonsRef.current[cat.id] = el; }}
              onClick={() => handleCategoryClick(cat.id)}
              className={`w-8 h-8 mx-auto mb-0.5 flex items-center justify-center rounded-lg transition-colors ${
                isSidebarCategoryActive(cat.id, activeCategoryRef.current, currentCategories[0]?.id)
                  ? 'ring-2 ring-blue-500 ring-inset text-blue-600'
                  : 'text-qc-fg-muted hover:bg-qc-hover'
              }`}
            >
              <i className={`ti ${cat.icon} text-base`}></i>
            </button>
          </Tooltip>
        ))}
      </div>

      {/* 主内容区 */}
      <div className="flex-1 flex flex-col overflow-hidden">

        {/* 内容滚动区 */}
        {showImages ? (
          <ImageLibraryTab
            ref={imageLibraryRef}
            currentGroup={currentImageGroup}
            imageGroups={imageGroups}
            searchQuery={searchQuery}
            onGroupsChange={refreshImageLibraryGroups}
            onImageDragStart={handleImagePluginDragStart}
            onImageDragEnd={handleImagePluginDragEnd}
            onImageDragCancel={clearImagePluginDragState}
            reloadKey={imageLibraryReloadKey}
          />
        ) : (
        <div ref={contentMeasureRef} className="emoji-content flex-1 overflow-hidden custom-scrollbar-container">
          {(!isReady || !isModeReady) ? (
            <div className="flex items-center justify-center h-32 text-qc-fg-subtle">
              <i className="ti ti-loader-2 animate-spin mr-2"></i>
              {t('common.loading')}
            </div>
          ) : (
            <Virtuoso
              key={`emoji-font-${useEmojiFallbackFont ? 'fallback' : 'system'}`}
              ref={scrollContainerRef}
              totalCount={virtualData.length}
              itemContent={renderVirtualItem}
              computeItemKey={(index) => virtualData[index]?.id || index}
              rangeChanged={handleRangeChanged}
              scrollerRef={scrollerRefCallback}
              overscan={10}
              className="h-full"
              style={{ height: '100%' }}
            />
          )}
        </div>
        )}
      </div>

      {/* 肤色选择器 */}
      {skinPickerEmoji && (
        <>
          <div className="fixed inset-0 z-[199]" onClick={() => setSkinPickerEmoji(null)} />
          <div 
            className="fixed z-[200] flex gap-1 p-1.5 bg-qc-panel rounded-xl shadow-xl border border-qc-border"
            onMouseLeave={() => setSkinPickerEmoji(null)}
            style={(() => {
              const { rect } = skinPickerEmoji;
              const pickerWidth = 220;
              const pickerHeight = 40;
              let left = rect.left + rect.width / 2 - pickerWidth / 2;
              let top = rect.top - pickerHeight - 8;
              if (top < 10) {
                top = rect.bottom + 8;
              }
              if (left < 10) left = 10;
              if (left + pickerWidth > window.innerWidth - 10) left = window.innerWidth - pickerWidth - 10;
              return { left, top };
            })()}
          >
            {skinPickerEmoji.variants.map((variant, i) => {
              const isCurrent = (i === 0 && skinTone === 'default') || (i > 0 && SKIN_TONES[i]?.id === skinTone);
              return (
                <Tooltip key={variant} content={SKIN_TONES[i]?.label || 'Default'} placement="top" asChild>
                  <button
                    onClick={() => {
                      handlePaste(skinPickerEmoji.item, skinPickerEmoji.catId, variant, skinPickerEmoji.baseChar);
                      setSkinPickerEmoji(null);
                    }}
                    className={`w-8 h-8 flex items-center justify-center text-xl rounded-lg transition-all hover:scale-110 ${
                      isCurrent
                        ? 'bg-blue-100 ring-2 ring-blue-500'
                        : 'hover:bg-qc-hover'
                    }`}
                  >
                    <span className={emojiGlyphClassName}>{variant}</span>
                  </button>
                </Tooltip>
              );
            })}
          </div>
        </>
      )}

      {showImageGroupModal && (
        <ImageGroupModal
          group={editingImageGroup}
          onClose={() => {
            setShowImageGroupModal(false);
            setEditingImageGroup(null);
          }}
          onSave={handleImageGroupSaved}
        />
      )}
    </div>
  );
});

export default EmojiTab;
