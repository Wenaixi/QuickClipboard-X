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

pub fn mouse_auto_popup_state() -> MouseAutoPopupState {
    WINDOW_STATE.read().mouse_auto_popup
}

pub fn start_mouse_auto_popup(deadline_ms: u64) -> u64 {
    let mut state = WINDOW_STATE.write();
    let session_id = state.mouse_auto_popup.session_id.saturating_add(1);
    state.mouse_auto_popup = MouseAutoPopupState::start(session_id, deadline_ms);
    session_id
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
    let session_id = state.mouse_auto_popup.session_id.saturating_add(1);
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
    let mut state = WINDOW_STATE.write();
    state.is_snapped = false;
    state.is_hidden = false;
    state.snap_edge = SnapEdge::None;
    state.snap_position = None;
    state.snap_monitor_id = None;
    state.snap_ratio = None;
    let session_id = state.mouse_auto_popup.session_id.saturating_add(1);
    state.mouse_auto_popup = MouseAutoPopupState::inactive_with_session(session_id);
}

pub fn set_pinned(is_pinned: bool) {
    WINDOW_STATE.write().is_pinned = is_pinned;
}

pub fn is_pinned() -> bool {
    WINDOW_STATE.read().is_pinned
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static SERIAL: Mutex<()> = Mutex::new(());

    fn lock_serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|error| error.into_inner())
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
}
