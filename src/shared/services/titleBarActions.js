import { setWindowPinned, openSettingsWindow } from '@shared/api'

let pinnedState = false

// 启动/主窗口重建后从后端同步置顶状态:低占用销毁重建主窗口时后端
// WINDOW_STATE 的 is_pinned 保留(会话级),但本模块 pinnedState 是页面
// 模块变量会随重建复位为 false——标题栏图标与实际置顶不一致。查询
// 后端真实状态覆盖本地初始值。
export async function syncWindowPinState() {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const pinned = Boolean(await invoke('get_window_pin_state'));
    if (pinned !== pinnedState) {
      pinnedState = pinned;
      emitPinStateChanged(pinned);
    }
    return pinned;
  } catch {
    return pinnedState;
  }
}

function emitPinStateChanged(state) {
  if (typeof window === 'undefined') {
    return
  }

  window.dispatchEvent(new CustomEvent('window-pin-state-changed', {
    detail: { pinned: state }
  }))
}

export function getWindowPinState() {
  return pinnedState
}

export async function toggleWindowPin() {
  const nextState = !pinnedState
  await setWindowPinned(nextState)
  pinnedState = nextState
  emitPinStateChanged(nextState)
  return nextState
}

export async function openAppSettings() {
  await openSettingsWindow()
}
