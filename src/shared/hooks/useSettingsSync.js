import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { settingsStore } from '@shared/store/settingsStore'
import i18n from '@shared/i18n'

// 监听设置变更事件并同步到当前窗口（跨窗口设置同步）
export function useSettingsSync() {
  useEffect(() => {
    let unlisten = null

    // 监听设置变更事件
    const setupListener = async () => {
      try {
        unlisten = await listen('settings-changed', (event) => {
          // 批量更新设置
          if (event.payload && typeof event.payload === 'object') {
            settingsStore.updateSettings(event.payload)

            if (event.payload.language !== undefined && event.payload.language !== i18n.language) {
              i18n.changeLanguage(event.payload.language)
            }
          }
        })
      } catch (error) {
        console.error('设置监听器启动失败:', error)
      }
    }

    setupListener()

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [])
}

