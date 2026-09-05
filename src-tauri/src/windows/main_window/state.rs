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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MainWindowShowSource {
    Explicit,
    MouseAuto,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseAutoPopupDecision {
    Hold,
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseEdgeTransition {
    Baseline,
    Enter,
    Leave,
    Stable,
}

pub const fn observe_mouse_edge(
    previous: Option<bool>,
    is_near: bool,
) -> (Option<bool>, MouseEdgeTransition) {
    match previous {
        None => (Some(is_near), MouseEdgeTransition::Baseline),
        Some(previous) if previous == is_near => {
            (Some(is_near), MouseEdgeTransition::Stable)
        }
        Some(true) => (Some(false), MouseEdgeTransition::Leave),
        Some(false) => (Some(true), MouseEdgeTransition::Enter),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavigationHotkeyDecision {
    Enable,
    Disable,
}

pub const fn navigation_hotkey_decision(
    source: MainWindowShowSource,
) -> NavigationHotkeyDecision {
    match source {
        MainWindowShowSource::Explicit => NavigationHotkeyDecision::Enable,
        MainWindowShowSource::MouseAuto => NavigationHotkeyDecision::Disable,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseAutoPopupState {
    pub active: bool,
    pub promoted: bool,
    pub session_id: u64,
    pub deadline_ms: u64,
}

impl MouseAutoPopupState {
    pub const fn inactive_with_session(session_id: u64) -> Self {
        Self {
            active: false,
            promoted: false,
            session_id,
            deadline_ms: 0,
        }
    }

    pub const fn start(session_id: u64, deadline_ms: u64) -> Self {
        Self {
            active: true,
            promoted: false,
            session_id,
            deadline_ms,
        }
    }

    pub const fn is_current(self, session_id: u64) -> bool {
        self.active && self.session_id == session_id
    }

    pub const fn is_expired(self, now_ms: u64) -> bool {
        self.active && !self.promoted && now_ms >= self.deadline_ms
    }

    pub const fn promote(self) -> Self {
        Self {
            active: self.active,
            promoted: true,
            session_id: self.session_id,
            deadline_ms: self.deadline_ms,
        }
    }

    pub const fn decision(self, now_ms: u64, pinned: bool) -> MouseAutoPopupDecision {
        if !self.active || self.promoted || pinned || now_ms < self.deadline_ms {
            MouseAutoPopupDecision::Hold
        } else {
            MouseAutoPopupDecision::Hide
        }
    }
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
    pub mouse_auto_popup: MouseAutoPopupState,
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
            mouse_auto_popup: MouseAutoPopupState::inactive_with_session(0),
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
    let mut state = WINDOW_STATE.write();
    if is_dragging && !state.is_dragging && state.mouse_auto_popup.active {
        let session_id = next_mouse_auto_popup_session_id(state.mouse_auto_popup.session_id);
        state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(session_id);
    }
    state.is_dragging = is_dragging;
}

pub fn set_snap_edge(
    edge: SnapEdge,
    position: Option<(i32, i32)>,
    monitor_id: Option<String>,
    ratio: Option<f64>,
) {
    let mut state = WINDOW_STATE.write();
    if edge == SnapEdge::None && state.mouse_auto_popup.active {
        state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(
            next_mouse_auto_popup_session_id(state.mouse_auto_popup.session_id),
        );
    }
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

pub fn mouse_auto_popup_state() -> MouseAutoPopupState {
    WINDOW_STATE.read().mouse_auto_popup
}

pub fn start_mouse_auto_popup(deadline_ms: u64) -> u64 {
    let mut state = WINDOW_STATE.write();
    let session_id = next_mouse_auto_popup_session_id(state.mouse_auto_popup.session_id);
    state.mouse_auto_popup = MouseAutoPopupState::start(session_id, deadline_ms);
    session_id
}

fn next_mouse_auto_popup_session_id(current: u64) -> u64 {
    current.checked_add(1).expect("自动弹出会话 ID 已耗尽")
}

pub fn promote_mouse_auto_popup_for_session(session_id: u64) -> bool {
    let mut state = WINDOW_STATE.write();
    if !state.mouse_auto_popup.is_current(session_id) {
        return false;
    }
    state.mouse_auto_popup = state.mouse_auto_popup.promote();
    true
}

pub fn clear_mouse_auto_popup_for_session(session_id: u64) -> bool {
    let mut state = WINDOW_STATE.write();
    if !state.mouse_auto_popup.is_current(session_id) {
        return false;
    }
    state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(session_id);
    true
}

pub fn invalidate_mouse_auto_popup() {
    let mut state = WINDOW_STATE.write();
    let session_id = next_mouse_auto_popup_session_id(state.mouse_auto_popup.session_id);
    state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(session_id);
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
    invalidate_mouse_auto_popup();
    let mut state = WINDOW_STATE.write();
    state.is_snapped = false;
    state.is_hidden = false;
    state.snap_edge = SnapEdge::None;
    state.snap_position = None;
    state.snap_monitor_id = None;
    state.snap_ratio = None;
}

pub fn set_pinned(is_pinned: bool) {
    let mut state = WINDOW_STATE.write();
    if state.is_pinned != is_pinned && state.mouse_auto_popup.active {
        let session_id = next_mouse_auto_popup_session_id(state.mouse_auto_popup.session_id);
        state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(session_id);
    }
    state.is_pinned = is_pinned;
}

pub fn is_pinned() -> bool {
    WINDOW_STATE.read().is_pinned
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::thread;

    static SERIAL: Mutex<()> = Mutex::new(());
    static SNAP_SOURCE: OnceLock<String> = OnceLock::new();

    fn snap_source() -> &'static str {
        SNAP_SOURCE.get_or_init(|| {
            std::fs::read_to_string(format!(
                "{}/src/windows/main_window/snap.rs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .expect("找不到 snap.rs 源文件")
        })
    }

    fn lock_serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn strip_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("
")
    }

    fn function_body(source: &str, name: &str, end_marker: &str) -> String {
        let start = source.find(name).expect("找不到函数定义");
        let after = &source[start..];
        let end = after.find(end_marker).unwrap_or(after.len());
        strip_line_comments(&after[..end])
    }

    #[test]
    fn hidden_accounting_is_atomic_under_concurrent_reads() {
        let _guard = lock_serial();
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden_and_window_state(false, WindowState::Visible);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_read = stop.clone();
        let observed_tear = Arc::new(AtomicBool::new(false));
        let tear_read = observed_tear.clone();
        let reader = thread::spawn(move || {
            while !stop_read.load(Ordering::Relaxed) {
                let state = get_window_state();
                if state.is_snapped && state.is_hidden && state.state != WindowState::Hidden {
                    tear_read.store(true, Ordering::Relaxed);
                }
            }
        });

        for _ in 0..10_000 {
            set_hidden_and_window_state(true, WindowState::Hidden);
            set_hidden_and_window_state(false, WindowState::Visible);
        }

        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        assert!(!observed_tear.load(Ordering::Relaxed));
    }

    #[test]
    fn refresh_writes_hidden_accounting_through_atomic_entry() {
        let body = function_body(
            snap_source(),
            "pub fn refresh_hidden_snapped_window",
            "
pub fn ",
        );
        assert!(body.contains(
            "set_hidden_and_window_state(true, super::state::WindowState::Hidden)"
        ));
        assert!(!body.contains("set_hidden("));
        assert!(!body.contains("set_window_state("));
    }

    #[test]
    fn refresh_rechecks_state_before_writing_hidden_back() {
        let body = function_body(
            snap_source(),
            "pub fn refresh_hidden_snapped_window",
            "
pub fn ",
        );
        let recheck = body
            .find("if super::state::get_window_state().is_hidden")
            .expect("refresh 必须先重查隐藏状态");
        let write = body
            .find("set_hidden_and_window_state(true, super::state::WindowState::Hidden)")
            .expect("refresh 必须有隐藏原子写入");
        assert!(recheck < write);
    }

    #[test]
    fn show_writes_visible_accounting_through_atomic_entry() {
        let body = function_body(
            snap_source(),
            "pub fn show_snapped_window",
            "
fn begin_animation",
        );
        assert!(body.contains(
            "set_hidden_and_window_state(false, super::state::WindowState::Visible)"
        ));
        assert!(!body.contains("set_hidden("));
        assert!(!body.contains("set_window_state("));
    }

    #[test]
    fn source_controls_navigation_hotkey_decision() {
        assert_eq!(
            navigation_hotkey_decision(MainWindowShowSource::MouseAuto),
            NavigationHotkeyDecision::Disable
        );
        assert_eq!(
            navigation_hotkey_decision(MainWindowShowSource::Explicit),
            NavigationHotkeyDecision::Enable
        );
    }

    #[test]
    fn first_near_sample_only_establishes_baseline() {
        assert_eq!(
            observe_mouse_edge(None, true),
            (Some(true), MouseEdgeTransition::Baseline)
        );
    }

    #[test]
    fn far_then_near_emits_entry_and_near_then_far_emits_leave() {
        assert_eq!(
            observe_mouse_edge(Some(false), true),
            (Some(true), MouseEdgeTransition::Enter)
        );
        assert_eq!(
            observe_mouse_edge(Some(true), false),
            (Some(false), MouseEdgeTransition::Leave)
        );
    }

    #[test]
    fn auto_popup_hides_at_deadline_without_interaction() {
        let popup = MouseAutoPopupState::start(7, 1_000);
        assert_eq!(popup.decision(999, false), MouseAutoPopupDecision::Hold);
        assert_eq!(popup.decision(1_000, false), MouseAutoPopupDecision::Hide);
    }

    #[test]
    fn interaction_or_pin_keeps_auto_popup_visible() {
        let popup = MouseAutoPopupState::start(7, 1_000);
        assert_eq!(
            popup.promote().decision(2_000, false),
            MouseAutoPopupDecision::Hold
        );
        assert_eq!(popup.decision(2_000, true), MouseAutoPopupDecision::Hold);
    }

    #[test]
    fn stale_session_is_not_current() {
        let popup = MouseAutoPopupState::start(7, 1_000);
        assert!(popup.is_current(7));
        assert!(!popup.is_current(8));
    }

    #[test]
    fn session_ids_are_monotonic_across_clear_and_invalidate() {
        let _guard = lock_serial();
        let first = start_mouse_auto_popup(1_000);
        assert!(clear_mouse_auto_popup_for_session(first));
        let second = start_mouse_auto_popup(2_000);
        assert!(second > first);
        invalidate_mouse_auto_popup();
        let third = start_mouse_auto_popup(3_000);
        assert!(third > second);
    }

    #[test]
    fn session_id_allocator_rejects_overflow_instead_of_reusing_id() {
        assert_eq!(next_mouse_auto_popup_session_id(u64::MAX - 1), u64::MAX);
        assert!(std::panic::catch_unwind(|| next_mouse_auto_popup_session_id(u64::MAX)).is_err());
    }

    #[test]
    fn clearing_snap_invalidates_popup_session() {
        let _guard = lock_serial();
        set_snap_edge(SnapEdge::Left, Some((0, 0)), None, Some(0.5));
        let session_id = start_mouse_auto_popup(1_000);
        clear_snap();
        assert!(!mouse_auto_popup_state().is_current(session_id));
        assert!(!get_window_state().is_snapped);
    }

    #[test]
    fn clearing_snap_calls_popup_invalidation_before_state_reset() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/state.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 state.rs 源文件");
        let body = source
            .split("pub fn clear_snap()")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("找不到 clear_snap 函数体");
        let invalidate = body
            .find("invalidate_mouse_auto_popup()")
            .expect("clear_snap 必须失效会话");
        let reset = body
            .find("state.is_snapped = false")
            .expect("clear_snap 必须清理贴边状态");
        assert!(invalidate < reset);
    }

    #[test]
    fn pin_change_invalidates_active_popup_session() {
        let _guard = lock_serial();
        let initial_pinned = get_window_state().is_pinned;
        set_pinned(!initial_pinned);
        let session_id = start_mouse_auto_popup(1_000);
        set_pinned(initial_pinned);
        assert!(!mouse_auto_popup_state().is_current(session_id));
    }

    #[test]
    fn pin_change_invalidation_only_advances_active_session() {
        let _guard = lock_serial();
        let initial_pinned = get_window_state().is_pinned;
        invalidate_mouse_auto_popup();
        let before = mouse_auto_popup_state().session_id;
        set_pinned(!initial_pinned);
        assert_eq!(mouse_auto_popup_state().session_id, before);
        set_pinned(initial_pinned);
    }

    #[test]
    fn starting_drag_invalidates_active_popup_session() {
        let _guard = lock_serial();
        let session_id = start_mouse_auto_popup(1_000);
        set_dragging(true);
        assert!(!mouse_auto_popup_state().is_current(session_id));
        set_dragging(false);
    }
}
