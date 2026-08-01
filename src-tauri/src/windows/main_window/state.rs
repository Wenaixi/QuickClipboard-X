use parking_lot::RwLock;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Hidden,
    Visible,
    Minimized,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapEdge {
    None,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct MainWindowState {
    pub state: WindowState,
    pub is_dragging: bool,
    pub is_snapped: bool,
    pub is_hidden: bool,
    pub is_pinned: bool,
    pub snap_edge: SnapEdge,
    pub snap_position: Option<(i32, i32)>,
    pub snap_monitor_id: Option<String>,
    pub snap_ratio: Option<f64>,
    pub clipboard_refresh_pending: bool,
    pub favorites_refresh_pending: bool,
    pub groups_refresh_pending: bool,
}

impl Default for MainWindowState {
    fn default() -> Self {
        Self {
            state: WindowState::Hidden,
            is_dragging: false,
            is_snapped: false,
            is_hidden: false,
            is_pinned: false,
            snap_edge: SnapEdge::None,
            snap_position: None,
            snap_monitor_id: None,
            snap_ratio: None,
            clipboard_refresh_pending: false,
            favorites_refresh_pending: false,
            groups_refresh_pending: false,
        }
    }
}

static WINDOW_STATE: Lazy<RwLock<MainWindowState>> = 
    Lazy::new(|| RwLock::new(MainWindowState::default()));

pub fn get_window_state() -> MainWindowState {
    WINDOW_STATE.read().clone()
}

pub fn set_window_state(state: WindowState) {
    WINDOW_STATE.write().state = state;
}

pub fn is_main_window_visible_for_updates() -> bool {
    let state = WINDOW_STATE.read();
    state.state == WindowState::Visible && !state.is_hidden
}

pub fn set_dragging(is_dragging: bool) {
    WINDOW_STATE.write().is_dragging = is_dragging;
}

pub fn set_snap_edge(
    edge: SnapEdge,
    position: Option<(i32, i32)>,
    monitor_id: Option<String>,
    ratio: Option<f64>,
) {
    let mut state = WINDOW_STATE.write();
    state.is_snapped = edge != SnapEdge::None;
    state.snap_edge = edge;
    state.snap_position = position;
    state.snap_monitor_id = monitor_id;
    state.snap_ratio = ratio;
}

pub fn set_hidden(is_hidden: bool) {
    WINDOW_STATE.write().is_hidden = is_hidden;
}

// 原子地同时写入隐藏标记与窗口状态,避免 hide_snapped_window 记账时
// 被并发读侧(热键/原始输入线程的 toggle)观察到撕裂中间态
// (is_hidden=true 但 state=Visible 会让 toggle 误判 should_show,导致 hide 静默失败)
pub fn set_hidden_and_window_state(is_hidden: bool, window_state: WindowState) {
    let mut state = WINDOW_STATE.write();
    state.is_hidden = is_hidden;
    state.state = window_state;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    // 回归:hide_snapped_window 的隐藏记账必须原子。
    // 并发读线程在写入期间不得观察到 is_hidden=true 但 state=Visible 的撕裂态,
    // 否则 toggle(热键/原始输入线程)会误判 should_show,让 hide 静默失败。
    #[test]
    fn hidden_accounting_is_atomic_under_concurrent_reads() {
        // 先置入与 hide 前的状态:is_snapped + is_hidden=false + state=Visible
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden(false);
        set_window_state(WindowState::Visible);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_read = stop.clone();
        let observed_tear = Arc::new(AtomicBool::new(false));
        let tear_read = observed_tear.clone();

        // 读线程:持续快照,检测撕裂态
        let reader = thread::spawn(move || {
            while !stop_read.load(Ordering::Relaxed) {
                let state = get_window_state();
                if state.is_snapped {
                    if state.is_hidden && state.state != WindowState::Hidden {
                        tear_read.store(true, Ordering::Relaxed);
                    }
                }
            }
        });

        // 写线程:反复原子写入
        for _ in 0..10000 {
            set_hidden_and_window_state(true, WindowState::Hidden);
            set_hidden_and_window_state(false, WindowState::Visible);
        }

        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        assert!(
            !observed_tear.load(Ordering::Relaxed),
            "并发读观察到撕裂中间态:is_hidden=true 但 state != Hidden"
        );
    }
}

pub fn mark_clipboard_refresh_pending() {
    WINDOW_STATE.write().clipboard_refresh_pending = true;
}

pub fn mark_favorites_refresh_pending() {
    let mut state = WINDOW_STATE.write();
    state.favorites_refresh_pending = true;
    state.clipboard_refresh_pending = true;
}

pub fn mark_groups_refresh_pending() {
    WINDOW_STATE.write().groups_refresh_pending = true;
}

pub fn take_pending_refresh_flags() -> (bool, bool, bool) {
    let mut state = WINDOW_STATE.write();
    let flags = (
        state.clipboard_refresh_pending,
        state.favorites_refresh_pending,
        state.groups_refresh_pending,
    );
    state.clipboard_refresh_pending = false;
    state.favorites_refresh_pending = false;
    state.groups_refresh_pending = false;
    flags
}

pub fn is_snapped() -> bool {
    WINDOW_STATE.read().is_snapped
}

pub fn clear_snap() {
    let mut state = WINDOW_STATE.write();
    state.is_snapped = false;
    state.is_hidden = false;
    state.snap_edge = SnapEdge::None;
    state.snap_position = None;
    state.snap_monitor_id = None;
    state.snap_ratio = None;
}

pub fn set_pinned(is_pinned: bool) {
    WINDOW_STATE.write().is_pinned = is_pinned;
}

pub fn is_pinned() -> bool {
    WINDOW_STATE.read().is_pinned
}

