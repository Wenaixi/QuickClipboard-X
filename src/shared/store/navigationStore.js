import { proxy } from 'valtio'

// 导航状态管理
export const navigationStore = proxy({
  // 当前选中的索引
  currentSelectedIndex: -1,
  // 是否处于导航模式
  navigationMode: false,
  // 是否正在使用键盘导航
  isKeyboardNavigation: false,
  // 是否正在滚动
  isScrolling: false,
  // 当前活动的标签页
  activeTab: 'clipboard',
  // 表情页 EmojiTab 是否激活键盘区域导航(进入 search/grid/sidebar 任一 zone 时为 true)
  // App.jsx 方向键 handler 据此决定是否丢弃(避免后端 navigate-up/down/tab-left/right 事件切主标签/切种类)
  emojiKbActive: false,

  // 设置当前选中索引
  setSelectedIndex(index) {
    this.currentSelectedIndex = index
    if (index >= 0) {
      this.navigationMode = true
    }
  },

  // 重置导航状态
  resetNavigation() {
    this.currentSelectedIndex = -1
    this.navigationMode = false
    this.isKeyboardNavigation = false
  },

  // 设置键盘导航模式
  setKeyboardNavigation(isKeyboard) {
    this.isKeyboardNavigation = isKeyboard
  },

  // 设置滚动状态
  setScrolling(isScrolling) {
    this.isScrolling = isScrolling
  },

  // 设置表情页键盘区域导航激活状态
  setEmojiKbActive(isActive) {
    this.emojiKbActive = isActive
  },

  // 设置当前标签页
  setActiveTab(tab) {
    this.activeTab = tab
    // 切换标签页时重置导航
    this.resetNavigation()
    // 切走 emoji tab 时强制关闭键盘激活(emoji tab 重新挂载会重置)
    if (tab !== 'emoji') {
      this.emojiKbActive = false
    }
  }
})

