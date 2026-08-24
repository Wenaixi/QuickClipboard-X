import '@tabler/icons-webfont/dist/tabler-icons.min.css';
import { useTranslation } from 'react-i18next';
import { useRef, useEffect, useLayoutEffect, useState, useCallback, forwardRef, useImperativeHandle } from 'react';
import { useSnapshot } from 'valtio';
import { settingsStore } from '@shared/store/settingsStore';
import { normalizeVisibleOptionalTabs } from '@shared/constants/tabVisibility';
import TabButton from './TabButton';
import FilterButton from './FilterButton';
import GroupsPopup from './GroupsPopup';
import Tooltip from '@shared/components/common/Tooltip.jsx';

const FILTER_BUTTON_SIZE = 28;
const FILTER_BUTTON_GAP = 4;
const GROUP_BUTTON_WIDTH = 60;
const GROUP_REVEAL_EDGE_ZONE = 24;
const GROUP_REVEAL_HIDE_DELAY = 300;
// 过滤器选项常量:App.jsx 过滤热键(⌘+←/→)也复用,避免硬编码数组漂移
export const FILTER_IDS = ['all', 'text', 'image', 'file', 'link'];
// emoji 子模式顺序常量:App.jsx 过滤热键与 handleKbNav 共用
export const EMOJI_MODE_IDS = ['emoji', 'symbols', 'images'];

function getCollapsedFilterWidth(filterCount, groupButtonWidth) {
  if (filterCount <= 0) {
    return groupButtonWidth;
  }

  return filterCount * FILTER_BUTTON_SIZE
    + (filterCount - 1) * FILTER_BUTTON_GAP
    + FILTER_BUTTON_GAP
    + groupButtonWidth;
}

function getVisibleFilterCountByWidth(width, groupButtonWidth) {
  // 最小显示 4 个:全部/文本/图片/链接 默认常驻,第 5 个(文件)折叠
  for (let count = FILTER_IDS.length; count >= 4; count -= 1) {
    if (width >= getCollapsedFilterWidth(count, groupButtonWidth)) {
      return count;
    }
  }

  return 4;
}

function measureIndicator(activeElement, containerElement) {
  if (!activeElement || !containerElement || !containerElement.contains(activeElement)) {
    return null;
  }

  const activeRect = activeElement.getBoundingClientRect();
  const containerRect = containerElement.getBoundingClientRect();
  return {
    width: activeRect.width,
    left: activeRect.left - containerRect.left
  };
}

function applyIndicatorPosition(indicatorElement, position) {
  if (!indicatorElement || !position) {
    return;
  }

  const width = `${position.width}px`;
  const left = `${position.left}px`;
  if (indicatorElement.style.width !== width) {
    indicatorElement.style.width = width;
  }
  if (indicatorElement.style.left !== left) {
    indicatorElement.style.left = left;
  }
}

function TabNavigation({
  activeTab,
  onTabChange,
  contentFilter,
  onFilterChange,
  pasteFilter = 'all',
  onPasteFilterChange,
  emojiMode,
  onEmojiModeChange,
  onGroupChange,
  groupsPopupRef,
  navigationMode = 'horizontal'
}, ref) {
  const {
    t
  } = useTranslation();
  const settings = useSnapshot(settingsStore);
  const uiAnimationEnabled = settings.uiAnimationEnabled !== false;
  const visibleOptionalTabs = normalizeVisibleOptionalTabs(settings.visibleOptionalTabs);
  const isSidebarLayout = navigationMode === 'sidebar';
  const isCompactFiltersLayout = compactFilters && !isSidebarLayout;
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
  const [isGroupsPanelOpen, setIsGroupsPanelOpen] = useState(false);
  const [isGroupButtonRevealed, setIsGroupButtonRevealed] = useState(false);
  const groupRevealTimerRef = useRef(null);
  const tabsRef = useRef({});
  const filtersRef = useRef({});
  const emojiModesRef = useRef({});
  const tabsContainerRef = useRef(null);
  const controlsContainerRef = useRef(null);
  const tabIndicatorRef = useRef(null);
  const controlsIndicatorRef = useRef(null);
  const rightAreaRef = useRef(null);
  const [tabAnimationKey, setTabAnimationKey] = useState(0);
  const [emojiModeAnimationKey, setEmojiModeAnimationKey] = useState(0);
  const [isFilterExpanded, setIsFilterExpanded] = useState(false);
  const [collapsedVisibleFilterCount, setCollapsedVisibleFilterCount] = useState(4);
  const [sidebarFixedWidth, setSidebarFixedWidth] = useState(null);
  const sidebarTabsMainRef = useRef(null);

  const allTabs = [{
    id: 'clipboard',
    label: t('clipboard.title') || '剪贴板',
    icon: 'ti ti-clipboard-text'
  }, {
    id: 'favorites',
    label: t('favorites.title') || '收藏',
    icon: 'ti ti-star'
  }, {
    id: 'emoji',
    label: t('emoji.title') || '符号',
    icon: 'ti ti-mood-smile'
  }];
  const tabs = allTabs.filter(tab => tab.id === 'clipboard' || visibleOptionalTabs.includes(tab.id));
  const horizontalTabAreaPercent = 35;
  const horizontalRightAreaPercent = 100 - horizontalTabAreaPercent;

  const emojiModes = [{
    id: 'emoji',
    label: t('emoji.emoji') || 'Emoji',
    icon: 'ti ti-mood-smile',
    emoji: '😀'
  }, {
    id: 'symbols',
    label: t('emoji.symbols') || '符号',
    icon: 'ti ti-math-symbols'
  }, {
    id: 'images',
    label: t('emoji.images') || '图片',
    icon: 'ti ti-photo-star'
  }];

  const filters = [{
    id: 'text',
    label: t('filter.text') || '文本',
    icon: "ti ti-file-text"
  }, {
    id: 'image',
    label: t('filter.image') || '图片',
    icon: "ti ti-photo"
  }, {
    id: 'file',
    label: t('filter.file') || '文件',
    icon: "ti ti-folder"
  }, {
    id: 'link',
    label: t('filter.link') || '链接',
    icon: "ti ti-link"
  }];

  const isFilterAutoExpanded = collapsedVisibleFilterCount >= 5;
  const expandableFilters = filters.slice(collapsedVisibleFilterCount);
  const shouldStretchHorizontalFilters = !isSidebarLayout;
  const shouldExpandFilters = isFilterAutoExpanded || isFilterExpanded;
  // F2 修:useFloatingExpandedFilters(!isFilterAutoExpanded && count<=2 && ...)在
  // collapsedVisibleFilterCount 最小 4 后(commit 6af73f0f 产品决策:全部/文本/
  // 图片/链接常驻,文件折叠)恒 false,浮动展开分支(878-903 死代码)已删。
  const shouldHideGroupButton = !isFilterAutoExpanded && shouldExpandFilters;
  const expandedExtraWidth = expandableFilters.length > 0
    ? expandableFilters.length * FILTER_BUTTON_SIZE + (expandableFilters.length - 1) * FILTER_BUTTON_GAP
    : 0;
  const groupButtonWidth = isSidebarLayout ? 92 : GROUP_BUTTON_WIDTH;
  const groupButtonVisible = isSidebarLayout || isGroupButtonRevealed || isGroupsPanelOpen;
  // F2 修:三连重复三元提取单变量(行为不变)
  const hideGroup = shouldHideGroupButton || !groupButtonVisible;
  const sidebarShowLabel = isSidebarLayout ? !isSidebarCollapsed : true;

  const updateTabIndicator = useCallback(() => {
    const activeElement = tabsRef.current[activeTab];
    const nextIndicator = measureIndicator(activeElement, tabsContainerRef.current);
    applyIndicatorPosition(tabIndicatorRef.current, nextIndicator);
  }, [activeTab]);

  const updateFilterIndicator = useCallback(() => {
    applyIndicatorPosition(controlsIndicatorRef.current, { width: 0, left: 0 });
  }, []);

  const updateEmojiModeIndicator = useCallback(() => {
    const activeElement = emojiModesRef.current[emojiMode];
    const nextIndicator = measureIndicator(activeElement, controlsContainerRef.current);
    applyIndicatorPosition(controlsIndicatorRef.current, nextIndicator);
  }, [emojiMode]);

  useEffect(() => {
    return () => {
      if (filterCollapseTimerRef.current) {
        clearTimeout(filterCollapseTimerRef.current);
        filterCollapseTimerRef.current = null;
      }
      if (groupRevealTimerRef.current) {
        clearTimeout(groupRevealTimerRef.current);
        groupRevealTimerRef.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (isSidebarLayout) {
      return undefined;
    }
    if (isGroupsPanelOpen) {
      setIsGroupButtonRevealed(true);
    }
    const timer = setTimeout(() => {
      setTabAnimationKey(prev => prev + 1);
    }, 300);
    return () => {
      clearTimeout(timer);
    };
  }, [activeTab, isSidebarLayout, isGroupsPanelOpen]);

  useEffect(() => {
    if (!isSidebarLayout) {
      setIsSidebarCollapsed(false);
    }
  }, [isSidebarLayout]);

  useEffect(() => {
    const timer = setTimeout(() => {
      setEmojiModeAnimationKey(prev => prev + 1);
    }, 300);
    return () => {
      clearTimeout(timer);
    };
  }, [emojiMode]);

  useEffect(() => {
    setIsFilterExpanded(false);
    setIsGroupButtonRevealed(false);
  }, [activeTab]);

  useEffect(() => {
    if (isSidebarLayout) {
      setCollapsedVisibleFilterCount(FILTER_IDS.length);
      return undefined;
    }

    if (activeTab === 'emoji') {
      setCollapsedVisibleFilterCount(4);
      return undefined;
    }

    const target = rightAreaRef.current;
    if (!target) {
      return undefined;
    }

    const updateAutoExpanded = () => {
      const width = target.clientWidth;
      const nextCollapsedVisibleCount = getVisibleFilterCountByWidth(width, groupButtonWidth);
      setCollapsedVisibleFilterCount(prev => (prev === nextCollapsedVisibleCount ? prev : nextCollapsedVisibleCount));
    };

    updateAutoExpanded();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateAutoExpanded);
      return () => {
        window.removeEventListener('resize', updateAutoExpanded);
      };
    }

    const observer = new ResizeObserver(() => {
      updateAutoExpanded();
    });
    observer.observe(target);
    return () => {
      observer.disconnect();
    };
  }, [activeTab, isSidebarLayout, groupButtonWidth]);

  useEffect(() => {
    if (isFilterAutoExpanded) {
      setIsFilterExpanded(false);
    }
  }, [isFilterAutoExpanded]);

  useLayoutEffect(() => {
    if (!isSidebarLayout) {
      setSidebarFixedWidth(null);
      return undefined;
    }

    const updateSidebarWidth = () => {
      const el = sidebarTabsMainRef.current;
      if (!el) return;
      const width = Math.ceil(el.getBoundingClientRect().width);
      if (Number.isFinite(width) && width > 0) {
        setSidebarFixedWidth(width);
      }
    };

    updateSidebarWidth();

    const target = sidebarTabsMainRef.current;
    if (typeof ResizeObserver !== 'undefined' && target) {
      const observer = new ResizeObserver(updateSidebarWidth);
      observer.observe(target);
      return () => observer.disconnect();
    }

    window.addEventListener('resize', updateSidebarWidth);
    return () => {
      window.removeEventListener('resize', updateSidebarWidth);
    };
  }, [isSidebarLayout, sidebarShowLabel, tabs.length]);

  useLayoutEffect(() => {
    if (isSidebarLayout) {
      return undefined;
    }

    let frameId = null;
    let disposed = false;

    const updateIndicators = () => {
      if (disposed) {
        return;
      }

      updateTabIndicator();
      updateFilterIndicator();
      updateEmojiModeIndicator();
    };

    const scheduleUpdate = () => {
      if (disposed) {
        return;
      }
      if (frameId !== null) {
        cancelAnimationFrame(frameId);
      }
      frameId = requestAnimationFrame(() => {
        frameId = null;
        updateIndicators();
      });
    };

    updateIndicators();

    let observer = null;
    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(scheduleUpdate);
      const observedElements = new Set();

      const addElementAndParents = (element, container) => {
        let current = element;
        while (current && container?.contains(current)) {
          observedElements.add(current);
          if (current === container) {
            break;
          }
          current = current.parentElement;
        }
      };

      const tabContainer = tabsContainerRef.current;
      const controlsContainer = controlsContainerRef.current;
      Object.values(tabsRef.current).forEach(element => addElementAndParents(element, tabContainer));
      Object.values(filtersRef.current).forEach(element => addElementAndParents(element, controlsContainer));
      Object.values(emojiModesRef.current).forEach(element => addElementAndParents(element, controlsContainer));
      if (tabContainer) observedElements.add(tabContainer);
      if (controlsContainer) observedElements.add(controlsContainer);

      observedElements.forEach(element => observer.observe(element));
    } else {
      window.addEventListener('resize', scheduleUpdate);
    }

    return () => {
      disposed = true;
      if (frameId !== null) {
        cancelAnimationFrame(frameId);
      }
      observer?.disconnect();
      if (!observer) {
        window.removeEventListener('resize', scheduleUpdate);
      }
    };
  }, [
    updateTabIndicator,
    updateFilterIndicator,
    updateEmojiModeIndicator,
    isSidebarLayout
  ]);

  const handleEmojiModeChange = (id) => {
    onEmojiModeChange(id);
  };

  // 分组按钮边缘悬停弹出:鼠标移到顶栏最右边缘时按钮滑出,离开后收回。
  // 按钮本身悬停(或面板打开)时保持显示,避免闪烁。
  const handleGroupRevealMouseMove = (event) => {
    if (isSidebarLayout || isGroupsPanelOpen || isGroupButtonRevealed) {
      // F2 修:悬停中(已 reveal)移回也必须取消挂起的隐藏定时器,
      // 否则按钮滑出→鼠标移出挂 300ms 定时器→定时器内移回→定时器到期按钮收起。
      if (groupRevealTimerRef.current) {
        clearTimeout(groupRevealTimerRef.current);
        groupRevealTimerRef.current = null;
      }
      return;
    }
    const rightArea = rightAreaRef.current;
    if (!rightArea) {
      return;
    }
    const rect = rightArea.getBoundingClientRect();
    if (rect.width <= 0) {
      return;
    }
    if (rect.right - event.clientX <= GROUP_REVEAL_EDGE_ZONE) {
      if (groupRevealTimerRef.current) {
        clearTimeout(groupRevealTimerRef.current);
        groupRevealTimerRef.current = null;
      }
      setIsGroupButtonRevealed(true);
    }
  };

  const handleGroupRevealMouseLeave = () => {
    if (isSidebarLayout || isGroupsPanelOpen) {
      return;
    }
    if (groupRevealTimerRef.current) {
      clearTimeout(groupRevealTimerRef.current);
    }
    groupRevealTimerRef.current = setTimeout(() => {
      setIsGroupButtonRevealed(false);
      groupRevealTimerRef.current = null;
    }, GROUP_REVEAL_HIDE_DELAY);
  };

  // controlsContainer 的 onMouseLeave 会同时被 filter 折叠与分组按钮收回使用。
  // 两个 handler 均无条件执行(无 activeTab 守卫):emoji 模式下 filter leave
  // 也会挂折叠定时器,但 emoji 模式无 filter onMouseEnter,isFilterExpanded
  // 恒 false,定时器到期同值短路,无害;分组收回独立挂在 onMouseLeave 上。
  const handleControlsContainerMouseLeave = (event) => {
    handleFilterAreaMouseLeave(event);
    handleGroupRevealMouseLeave();
  };

  const handleFilterAreaMouseEnter = () => {
    if (isFilterAutoExpanded) {
      return;
    }
    if (filterCollapseTimerRef.current) {
      clearTimeout(filterCollapseTimerRef.current);
      filterCollapseTimerRef.current = null;
    }
    setIsFilterExpanded(true);
  };

  const handlePasteFilterChange = id => {
    const nextFilters = isPasteFilterSelected(id)
      ? selectedPasteFilters.filter(filterId => filterId !== id)
      : [...selectedPasteFilters, id];
    onPasteFilterChange(nextFilters.join(',') || 'all');
  };

  const renderSidebarButton = ({
    id,
    label,
    icon,
    emoji,
    isActive,
    onClick,
    buttonRef,
    showLabel = true
  }) => {
    const handleClick = () => {
      onClick(id);
    };

    return (
      <div ref={buttonRef} className={showLabel ? 'relative inline-flex h-9 w-full' : 'relative inline-flex h-9 w-10'}>
        <Tooltip content={label} placement="right" asChild>
          <button
            onClick={handleClick}
            className={`
              relative z-10 flex items-center h-9 rounded-lg
              ${showLabel ? 'justify-start gap-2 px-3 w-full min-w-0 whitespace-nowrap' : 'justify-center gap-0 px-0 w-10'}
              focus:outline-none
              ${uiAnimationEnabled ? 'hover:scale-[1.01]' : ''}
              ${isActive
                ? 'bg-blue-500 text-white shadow-md hover:bg-blue-500'
                : 'text-qc-fg-muted hover:bg-qc-hover'}
            `}
            style={uiAnimationEnabled ? {
              transitionProperty: 'transform, box-shadow, background-color, color',
              transitionDuration: '200ms, 200ms, 500ms, 500ms'
            } : {}}
          >
            {emoji ? <span style={{ fontSize: 16 }}>{emoji}</span> : <i className={icon} style={{ fontSize: 16 }} />}
            {showLabel && (
              <span className="text-[12px] font-medium leading-none truncate flex-1 min-w-0 text-left">
                {label}
              </span>
            )}
          </button>
        </Tooltip>
      </div>
    );
  };

  if (isSidebarLayout) {
    return <div className="tab-navigation flex-shrink-0 h-full w-fit min-w-fit bg-qc-panel shadow-sm transition-colors duration-500 tab-bar">
      <div className="flex h-full min-h-0 w-fit">
        <div
          className="flex h-full min-h-0 flex-col border-r border-qc-border"
          style={sidebarFixedWidth ? { width: `${sidebarFixedWidth}px`, minWidth: `${sidebarFixedWidth}px` } : undefined}
        >
          <div ref={sidebarTabsMainRef} className="grid grid-cols-[max-content] gap-1 p-2 pb-2 w-max justify-items-stretch">
            {tabs.map((tab, index) => (
              <TabButton
                key={tab.id}
                id={tab.id}
                label={tab.label}
                icon={tab.icon}
                badgeCount={0}
                isActive={activeTab === tab.id}
                onClick={onTabChange}
                index={index}
                buttonRef={el => {
                  tabsRef.current[tab.id] = el;
                }}
                navigationMode="sidebar"
                showLabel={sidebarShowLabel}
              />
            ))}
          </div>

          <div
            className="mx-2 h-px shrink-0"
            style={{ backgroundColor: 'var(--bg-titlebar-border, var(--qc-border-strong))', opacity: 0.95 }}
          />

          <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2 w-full min-w-0">
            <div className="grid grid-cols-1 gap-1 w-full min-w-0 justify-items-stretch">
              {activeTab === 'emoji'
                    ? emojiModes.map(mode => renderSidebarButton({
                        id: mode.id,
                        label: mode.label,
                        icon: mode.icon,
                        emoji: mode.emoji,
                        isActive: emojiMode === mode.id,
                        onClick: handleEmojiModeChange,
                        showLabel: sidebarShowLabel,
                        buttonRef: el => {
                          emojiModesRef.current[mode.id] = el;
                        }
                      }))
                : filters.map(filter => renderSidebarButton({
                    id: filter.id,
                    label: filter.label,
                    icon: filter.icon,
                    isActive: pasteFilters.some(item => item.id === filter.id)
                      ? isPasteFilterSelected(filter.id)
                      : isFilterSelected(filter.id),
                    onClick: pasteFilters.some(item => item.id === filter.id)
                      ? handlePasteFilterChange
                      : handleFilterChange,
                    showLabel: sidebarShowLabel,
                    buttonRef: el => {
                      filtersRef.current[filter.id] = el;
                    }
                  }))
              }
            </div>
          </div>


            <>
              <div
                className="mx-2 h-px shrink-0"
                style={{ backgroundColor: 'var(--bg-titlebar-border, var(--qc-border-strong))', opacity: 0.95 }}
              />
              <div className="px-2 py-2">
                <Tooltip content="分组" placement="right" asChild>
                  <button
                    type="button"
                    onClick={() => groupsPopupRef.current?.togglePopup?.()}
                    className={`relative z-10 flex items-center h-9 rounded-lg focus:outline-none transition-all duration-200 ${
                      sidebarShowLabel
                        ? `justify-start gap-2 px-3 w-full ${
                            isGroupsPanelOpen
                              ? 'qc-active-icon-button bg-[var(--qc-accent)] text-[var(--qc-accent-fg)] shadow-md hover:bg-[var(--qc-accent)]'
                              : 'text-qc-fg-muted hover:bg-qc-hover'
                          }`
                        : `justify-start gap-2 px-3 w-10 overflow-hidden ${
                            isGroupsPanelOpen
                              ? 'qc-active-icon-button bg-[var(--qc-accent)] text-[var(--qc-accent-fg)] shadow-md hover:bg-[var(--qc-accent)]'
                              : 'text-qc-fg-muted hover:bg-qc-hover'
                          }`
                    }`}
                  >
                    <i className="ti ti-folders" style={{ fontSize: 16 }} />
                    {sidebarShowLabel && (
                      <span className="text-[12px] font-medium leading-none whitespace-nowrap">
                        分组
                      </span>
                    )}
                  </button>
                </Tooltip>
              </div>
            </>

          <div className="mt-auto p-2 pt-1">
            <Tooltip content={sidebarShowLabel ? '收起侧边栏' : '展开侧边栏'} placement="right" asChild>
              <button
                type="button"
                onClick={() => setIsSidebarCollapsed(prev => !prev)}
                className={`relative z-10 flex items-center h-9 rounded-lg focus:outline-none transition-all duration-200 ${
                  sidebarShowLabel
                    ? 'justify-start gap-2 px-3 w-full text-qc-fg-muted hover:bg-qc-hover'
                    : 'justify-start gap-2 px-3 w-10 text-qc-fg-muted hover:bg-qc-hover overflow-hidden'
                }`}
              >
                <i
                  className={isSidebarCollapsed ? 'ti ti-layout-sidebar-right-expand' : 'ti ti-layout-sidebar-left-collapse'}
                  style={{ fontSize: 16 }}
                />
                {sidebarShowLabel && (
                  <span className="text-[12px] font-medium leading-none whitespace-nowrap">
                    收起
                  </span>
                )}
              </button>
            </Tooltip>
          </div>
        </div>

        <div className="flex h-full min-h-0 shrink-0">
          <GroupsPopup
            ref={groupsPopupRef}
            activeTab={activeTab}
            onTabChange={onTabChange}
            onGroupChange={onGroupChange}
            onOpenChange={setIsGroupsPanelOpen}
            mode="sidebar"
          />
        </div>
      </div>
    </div>;
  }

  return <div className={`tab-navigation flex-shrink-0 bg-qc-panel shadow-sm transition-colors duration-500 tab-bar ${
    isSidebarLayout
      ? 'w-[190px] min-w-[190px] h-full border-r border-qc-border'
      : 'border-b border-qc-border'
  }`}>
    <div className={isSidebarLayout ? 'flex h-full min-h-0 flex-col' : 'flex items-stretch h-9 whitespace-nowrap'}>
      <div
        className={isSidebarLayout ? 'flex flex-col gap-1 p-2 pb-1' : 'flex items-center px-2 relative min-w-0'}
        style={!isSidebarLayout ? {
          flex: `0 0 calc(${horizontalTabAreaPercent}% - 1px)`
        } : undefined}
      >
        <div ref={tabsContainerRef} className={isSidebarLayout ? 'flex flex-col gap-1 w-full' : 'flex items-center justify-center gap-1 w-full relative'}>
          {!isSidebarLayout && (
            <div ref={tabIndicatorRef} className={`absolute left-0 w-0 rounded-lg pointer-events-none ${uiAnimationEnabled ? 'transition-all duration-300 ease-out' : ''}`} style={{
              height: '28px',
              top: '50%',
              transform: 'translateY(-50%)'
            }}>
              <div key={`tab-bounce-${tabAnimationKey}`} className={`w-full h-full rounded-lg bg-[var(--qc-accent)] ${uiAnimationEnabled ? 'animate-button-bounce' : ''}`} />
            </div>
          )}
          {tabs.map((tab, index) => (
            <TabButton
              key={tab.id}
              id={tab.id}
              label={tab.label}
              icon={tab.icon}
              badgeCount={0}
              isActive={activeTab === tab.id}
              onClick={onTabChange}
              index={index}
              buttonRef={el => {
                tabsRef.current[tab.id] = el;
              }}
              navigationMode={isSidebarLayout ? 'sidebar' : 'horizontal'}
            />
          ))}
        </div>
      </div>

      {!isSidebarLayout && (
        <div
          className="w-[1.5px] my-1.5 shrink-0"
          style={{ backgroundColor: 'var(--bg-titlebar-border, var(--qc-border-strong))', opacity: 0.95 }}
        />
      )}

      <div
        ref={rightAreaRef}
        className={isSidebarLayout ? 'tab-navigation-right flex-1 flex items-end px-2 pb-2 relative min-w-0' : 'tab-navigation-right flex items-center pl-1 pr-1 relative min-w-0'}
        style={!isSidebarLayout ? {
          flex: `0 0 calc(${horizontalRightAreaPercent}% - 1px)`
        } : undefined}
      >
        <div
          ref={controlsContainerRef}
          className={`flex min-w-0 max-w-full items-center gap-1 relative overflow-visible ${
            activeTab === 'emoji' || isFilterAutoExpanded
              ? 'w-full justify-center'
              : 'w-full'
          }`}
          onMouseLeave={handleControlsContainerMouseLeave}
          onMouseMove={handleGroupRevealMouseMove}
        >
          {!isSidebarLayout && (
            <div ref={controlsIndicatorRef} className={`absolute left-0 w-0 rounded-lg pointer-events-none ${uiAnimationEnabled ? 'transition-all duration-300 ease-out' : ''}`} style={{
              height: '28px',
              top: '50%',
              transform: 'translateY(-50%)'
            }}>
              <div key={`emoji-mode-bounce-${emojiModeAnimationKey}`} className={`w-full h-full rounded-lg bg-[var(--qc-accent)] ${uiAnimationEnabled ? 'animate-button-bounce' : ''}`} />
            </div>
          )}
          {activeTab === 'emoji'
            ? emojiModes.map(mode => (
                <div key={mode.id} ref={el => {
                  emojiModesRef.current[mode.id] = el;
                }} className="relative flex-1 h-7">
                  <Tooltip content={mode.label} placement={isSidebarLayout ? 'right' : 'bottom'} asChild>
                    <button
                      onClick={() => handleEmojiModeChange(mode.id)}
                      className={`relative z-10 flex items-center justify-center w-full h-full rounded-lg focus:outline-none ${uiAnimationEnabled ? 'hover:scale-105' : ''} ${
                        emojiMode === mode.id
                          ? 'qc-active-icon-button bg-[var(--qc-accent)] text-[var(--qc-accent-fg)] shadow-md hover:bg-[var(--qc-accent)]'
                          : 'text-qc-fg-muted hover:bg-qc-hover'
                      }`}
                      style={uiAnimationEnabled ? {
                        transitionProperty: 'transform, box-shadow, background-color, color',
                        transitionDuration: '200ms, 200ms, 500ms, 500ms'
                      } : {}}
                    >
                      {mode.emoji ? <span style={{ fontSize: 16 }}>{mode.emoji}</span> : <i className={mode.icon} style={{ fontSize: 16 }} />}
                    </button>
                  </Tooltip>
                </div>
              ))
            : (
                <>
                  {isFilterAutoExpanded ? (
                    <div className="flex items-center gap-1 flex-1 min-w-0" onMouseEnter={handleFilterAreaMouseEnter}>
                      {filters.map(filter => (
                        <FilterButton
                          key={filter.id}
                          id={filter.id}
                          label={filter.label}
                          icon={filter.icon}
                          isActive={contentFilter === filter.id}
                          onClick={onFilterChange}
                          stretch={shouldStretchHorizontalFilters}
                          buttonRef={el => {
                            filtersRef.current[filter.id] = el;
                          }}
                        />
                      ))}
                    </div>
                  ) : (
                    <div className="relative flex items-center gap-1 flex-1 min-w-0" onMouseEnter={handleFilterAreaMouseEnter}>
                      {filters.slice(0, collapsedVisibleFilterCount).map(filter => (
                        <FilterButton
                          key={filter.id}
                          id={filter.id}
                          label={filter.label}
                          icon={filter.icon}
                          isActive={contentFilter === filter.id}
                          onClick={onFilterChange}
                          stretch={shouldStretchHorizontalFilters}
                          buttonRef={el => {
                            filtersRef.current[filter.id] = el;
                          }}
                        />
                      ))}

                        <div
                          className={`flex items-center gap-1 overflow-hidden shrink-0 min-w-0 ${uiAnimationEnabled ? 'transition-all duration-300 ease-out' : ''}`}
                          style={{
                            width: shouldExpandFilters ? `${expandedExtraWidth}px` : '0px',
                            opacity: shouldExpandFilters ? 1 : 0,
                            pointerEvents: shouldExpandFilters ? 'auto' : 'none'
                          }}
                        >
                          {expandableFilters.map(filter => (
                            <FilterButton
                              key={filter.id}
                              id={filter.id}
                              label={filter.label}
                              icon={filter.icon}
                              isActive={contentFilter === filter.id}
                              onClick={onFilterChange}
                              stretch={false}
                              buttonRef={el => {
                                filtersRef.current[filter.id] = el;
                              }}
                            />
                          ))}
                        </div>
                    </div>
                  )}

                  <div
                    className={`overflow-visible shrink-0 ${uiAnimationEnabled ? 'transition-all duration-300 ease-out' : ''}`}
                    style={{
                      width: hideGroup ? '0px' : `${groupButtonWidth}px`,
                      opacity: hideGroup ? 0 : 1,
                      pointerEvents: hideGroup ? 'none' : 'auto'
                    }}
                  >
                    <GroupsPopup
                      ref={groupsPopupRef}
                      activeTab={activeTab}
                      onTabChange={onTabChange}
                      onGroupChange={onGroupChange}
                      onOpenChange={setIsGroupsPanelOpen}
                      mode={isSidebarLayout ? 'tab-sidebar' : 'tab'}
                    />
                  </div>
                </>
              )
          }
        </div>
      </div>
    </div>
  </div>;
}
export default forwardRef(TabNavigation);
