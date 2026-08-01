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
        set_hidden_and_window_state(false, WindowState::Visible);

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

    // refresh 形态决策后,is_hidden 与 WindowState 必须原子一致
    #[test]
    fn refresh_accounting_keeps_is_hidden_and_state_atomic() {
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden_and_window_state(true, WindowState::Hidden);
        let state = get_window_state();
        assert!(state.is_hidden, "refresh 后必须 is_hidden=true");
        assert_eq!(
            state.state,
            WindowState::Hidden,
            "refresh 后 WindowState 必须为 Hidden"
        );
        assert!(
            state.is_hidden && state.state == WindowState::Hidden,
            "is_hidden 与 state 不得出现撕裂态"
        );
    }

    // 任何路径若绕过原子入口单独写 is_hidden(不写 state),会立刻变红
    #[test]
    fn non_atomic_set_hidden_can_tear_with_visible_state() {
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden_and_window_state(true, WindowState::Hidden);
        let state = get_window_state();
        assert!(
            state.is_hidden && state.state == WindowState::Hidden,
            "原子写后 is_hidden 与 state 必须一致,不得撕裂"
        );
    }

    // refresh 末尾必须 re-check 状态,尊重并发 show 的写入,不反手覆盖
    #[test]
    fn refresh_state_write_must_recheck_concurrent_change() {
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        let entry_is_hidden = get_window_state().is_hidden;
        // 并发 show 抢占写 false/Visible
        set_hidden_and_window_state(false, WindowState::Visible);
        // refresh 末尾 re-check:中途已变 false,必须不再写回 true
        if entry_is_hidden == get_window_state().is_hidden {
            set_hidden_and_window_state(true, WindowState::Hidden);
        }
        let state = get_window_state();
        assert!(
            !state.is_hidden && state.state == WindowState::Visible,
            "refresh 末尾必须尊重并发 show 的写入,不得反手覆盖。\
             现状 is_hidden={} state={:?}",
            state.is_hidden,
            state.state
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

