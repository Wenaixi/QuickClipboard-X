import { useEffect, useRef } from 'react'
import { listen } from '@tauri-apps/api/event'

// 全局键盘导航Hook
export function useNavigationKeyboard({
  onNavigateUp = null,
  onNavigateDown = null,
  onExecuteItem = null,
  onTabLeft = null,
  onTabRight = null,
  onToggleSearch = null,
  onTogglePin = null,
  onPreviousGroup = null,
  onNextGroup = null,
  onFilterLeft = null,
  onFilterRight = null,
  onHideWindow = null,
  enabled = true
}) {
  const handlersRef = useRef({
    onNavigateUp,
    onNavigateDown,
    onExecuteItem,
    onTabLeft,
    onTabRight,
    onToggleSearch,
    onTogglePin,
    onPreviousGroup,
    onNextGroup,
    onFilterLeft,
    onFilterRight,
    onHideWindow
  })

  useEffect(() => {
    handlersRef.current = {
      onNavigateUp,
      onNavigateDown,
      onExecuteItem,
      onTabLeft,
      onTabRight,
      onToggleSearch,
      onTogglePin,
      onPreviousGroup,
      onNextGroup,
      onFilterLeft,
      onFilterRight,
      onHideWindow
    }
  }, [
    onNavigateUp,
    onNavigateDown,
    onExecuteItem,
    onTabLeft,
    onTabRight,
    onToggleSearch,
    onTogglePin,
    onPreviousGroup,
    onNextGroup,
    onFilterLeft,
    onFilterRight,
    onHideWindow
  ])

  useEffect(() => {
    if (!enabled) return
    
    let unlistenNavigationAction = null
    let cancelled = false
    
    const setupNavigationListener = async () => {
      try {
        const unlisten = await listen('navigation-action', (event) => {
          const action = event.payload.action
          const handlers = handlersRef.current
          
          switch (action) {
            case 'navigate-up':
              if (handlers.onNavigateUp) handlers.onNavigateUp()
              break
            case 'navigate-down':
              if (handlers.onNavigateDown) handlers.onNavigateDown()
              break
            case 'execute-item':
              if (handlers.onExecuteItem) handlers.onExecuteItem()
              break
            case 'tab-left':
              if (handlers.onTabLeft) handlers.onTabLeft()
              break
            case 'tab-right':
              if (handlers.onTabRight) handlers.onTabRight()
              break
            case 'filter-left':
              if (handlers.onFilterLeft) handlers.onFilterLeft()
              break
            case 'filter-right':
              if (handlers.onFilterRight) handlers.onFilterRight()
              break
            case 'focus-search':
              if (handlers.onToggleSearch) handlers.onToggleSearch()
              break
            case 'hide-window':
              // 走回调让 App 侧可加 isSearchFocused 守卫(搜索框聚焦时 Esc 应只动输入框)
              if (handlers.onHideWindow) {
                handlers.onHideWindow()
              }
              break
            case 'toggle-pin':
              if (handlers.onTogglePin) {
                handlers.onTogglePin()
              }
              break
            case 'previous-group':
              if (handlers.onPreviousGroup) handlers.onPreviousGroup()
              break
            case 'next-group':
              if (handlers.onNextGroup) handlers.onNextGroup()
              break
            default:
              break
          }
        })

        if (cancelled) {
          unlisten()
          return
        }

        unlistenNavigationAction = unlisten
      } catch (error) {
        console.error('设置导航监听器失败:', error)
      }
    }
    
    setupNavigationListener()
    
    return () => {
      cancelled = true
      if (unlistenNavigationAction) {
        unlistenNavigationAction()
        unlistenNavigationAction = null
      }
    }
  }, [enabled])
}
