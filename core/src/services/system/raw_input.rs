//! 原始输入最小支撑面：粘贴模拟注入标记与物理修饰键读取。
//!
//! 原实现（src-tauri 侧）含完整 raw input 钩子链路，本轮只裁剪出
//! paste/keyboard.rs 需要的两个 API。物理修饰键直接读 GetAsyncKeyState，
//! 不依赖钩子线程状态——注入键（SendInput 带 PASTE_INPUT_MARKER）
//! 不改变物理键，读真实物理/合成键当前状态即可消除粘滞漂移。

/// 粘贴注入键标记：SendInput 注入的键带此 dwExtraInfo，
/// 消费方据其识别并忽略合成按键，避免自触发循环。
#[cfg(target_os = "windows")]
pub(crate) const PASTE_INPUT_MARKER: usize = 0x5143_4C50;

/// 读取五个修饰键的物理实时状态（Ctrl/Shift/Alt/LWin/RWin）。
#[cfg(target_os = "windows")]
pub fn get_physical_modifier_keys_state() -> Option<(bool, bool, bool, bool, bool)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    unsafe {
        let is_down = |vk| (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0;
        Some((
            is_down(VK_CONTROL),
            is_down(VK_SHIFT),
            is_down(VK_MENU),
            is_down(0x5B), // VK_LWIN
            is_down(0x5C), // VK_RWIN
        ))
    }
}