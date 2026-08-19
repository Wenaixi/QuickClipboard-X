use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Selecting,
    Processing,
    Committing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorRect {
    pub fn new(left: i32, top: i32, width: u32, height: u32) -> Result<Self, SessionError> {
        if width == 0 || height == 0 {
            return Err(SessionError::InvalidMonitorRect);
        }
        Ok(Self { left, top, width, height })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotSession {
    session_id: String,
    monitor: MonitorRect,
    quick_action: bool,
    phase: SessionPhase,
    temp_files: Vec<PathBuf>,
    main_window_hidden_revision: Option<MainWindowVisibilityRevision>,
}

impl ScreenshotSession {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn monitor(&self) -> MonitorRect {
        self.monitor
    }

    pub fn quick_action(&self) -> bool {
        self.quick_action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainWindowVisibilityRevision(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPlan {
    pub session_id: String,
    pub temp_files: Vec<PathBuf>,
    pub hide_overlay: bool,
    pub restore_main_window: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartSessionResult {
    Started(ScreenshotSession),
    Existing { session_id: String, phase: SessionPhase },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    InvalidMonitorRect,
    NoActiveSession,
    StaleSession { expected: String, actual: String },
    CommitInProgress { session_id: String },
    InvalidTransition { session_id: String, from: SessionPhase, to: SessionPhase },
}

#[derive(Debug, Default)]
pub struct ScreenshotSessionManager {
    current: Option<ScreenshotSession>,
}

impl ScreenshotSessionManager {
    pub fn start(&mut self, monitor: MonitorRect, quick_action: bool) -> StartSessionResult {
        if let Some(session) = &self.current {
            return StartSessionResult::Existing {
                session_id: session.session_id.clone(),
                phase: session.phase,
            };
        }

        let session = ScreenshotSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            monitor,
            quick_action,
            phase: SessionPhase::Selecting,
            temp_files: Vec::new(),
            main_window_hidden_revision: None,
        };
        self.current = Some(session.clone());
        StartSessionResult::Started(session)
    }

    pub fn phase(&self) -> Option<SessionPhase> {
        self.current.as_ref().map(|session| session.phase)
    }

    pub fn current(&self) -> Option<&ScreenshotSession> {
        self.current.as_ref()
    }

    pub fn is_current_phase(&self, session_id: &str, phase: SessionPhase) -> bool {
        self.current.as_ref().is_some_and(|session| {
            session.session_id == session_id && session.phase == phase
        })
    }

    pub fn begin_processing(&mut self, session_id: &str) -> Result<(), SessionError> {
        let session = self.current.as_mut().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase != SessionPhase::Selecting {
            return Err(SessionError::InvalidTransition {
                session_id: session.session_id.clone(),
                from: session.phase,
                to: SessionPhase::Processing,
            });
        }
        session.phase = SessionPhase::Processing;
        Ok(())
    }

    pub fn register_temp_file(&mut self, session_id: &str, path: PathBuf) -> Result<(), SessionError> {
        let session = self.current.as_mut().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase != SessionPhase::Processing {
            return Err(SessionError::InvalidTransition {
                session_id: session.session_id.clone(),
                from: session.phase,
                to: SessionPhase::Processing,
            });
        }
        session.temp_files.push(path);
        Ok(())
    }

    pub fn begin_commit(&mut self, session_id: &str) -> Result<(), SessionError> {
        let session = self.current.as_mut().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase != SessionPhase::Processing {
            return Err(SessionError::InvalidTransition {
                session_id: session.session_id.clone(),
                from: session.phase,
                to: SessionPhase::Committing,
            });
        }
        session.phase = SessionPhase::Committing;
        Ok(())
    }

    pub fn finish_and_retain_file(
        &mut self,
        session_id: &str,
        current_visibility_revision: MainWindowVisibilityRevision,
        retained_file: &PathBuf,
    ) -> Result<CleanupPlan, SessionError> {
        let session = self.current.as_mut().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase != SessionPhase::Committing {
            return Err(SessionError::InvalidTransition {
                session_id: session.session_id.clone(),
                from: session.phase,
                to: SessionPhase::Committing,
            });
        }
        session.temp_files.retain(|path| path != retained_file);
        self.take_cleanup(session_id, current_visibility_revision)
    }

    pub fn mark_main_window_hidden(
        &mut self,
        session_id: &str,
        revision: MainWindowVisibilityRevision,
    ) -> Result<(), SessionError> {
        let session = self.current.as_mut().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        session.main_window_hidden_revision = Some(revision);
        Ok(())
    }

    pub fn cancel(
        &mut self,
        session_id: &str,
        current_visibility_revision: MainWindowVisibilityRevision,
    ) -> Result<CleanupPlan, SessionError> {
        let session = self.current.as_ref().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase == SessionPhase::Committing {
            return Err(SessionError::CommitInProgress {
                session_id: session.session_id.clone(),
            });
        }
        self.take_cleanup(session_id, current_visibility_revision)
    }

    pub fn finish(
        &mut self,
        session_id: &str,
        current_visibility_revision: MainWindowVisibilityRevision,
    ) -> Result<CleanupPlan, SessionError> {
        let session = self.current.as_ref().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        if session.phase != SessionPhase::Committing {
            return Err(SessionError::InvalidTransition {
                session_id: session.session_id.clone(),
                from: session.phase,
                to: SessionPhase::Committing,
            });
        }
        self.take_cleanup(session_id, current_visibility_revision)
    }

    fn take_cleanup(
        &mut self,
        session_id: &str,
        current_visibility_revision: MainWindowVisibilityRevision,
    ) -> Result<CleanupPlan, SessionError> {
        let session = self.current.as_ref().ok_or(SessionError::NoActiveSession)?;
        ensure_current_session(&session.session_id, session_id)?;
        let session = self.current.take().expect("active session checked");
        Ok(CleanupPlan {
            session_id: session.session_id,
            temp_files: session.temp_files,
            hide_overlay: true,
            restore_main_window: session.main_window_hidden_revision == Some(current_visibility_revision),
        })
    }
}

fn ensure_current_session(expected: &str, actual: &str) -> Result<(), SessionError> {
    if expected == actual {
        return Ok(());
    }
    Err(SessionError::StaleSession {
        expected: expected.to_string(),
        actual: actual.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn starts_one_selecting_session_and_retrigger_returns_existing_session() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(-1920, 0, 1920, 1080).unwrap();

        let first = manager.start(monitor, true);
        let session_id = match first {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };
        assert_eq!(manager.phase(), Some(SessionPhase::Selecting));

        let second = manager.start(monitor, true);
        assert_eq!(
            second,
            StartSessionResult::Existing {
                session_id: session_id.clone(),
                phase: SessionPhase::Selecting,
            }
        );
        assert_eq!(manager.current().unwrap().session_id(), session_id);
    }

    #[test]
    fn only_current_session_can_enter_processing_and_stale_completion_is_rejected() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 3840, 2160).unwrap();
        let session_id = match manager.start(monitor, false) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };

        assert_eq!(
            manager.begin_processing("expired-session"),
            Err(SessionError::StaleSession {
                expected: session_id.clone(),
                actual: "expired-session".to_string(),
            })
        );
        assert_eq!(manager.phase(), Some(SessionPhase::Selecting));

        manager.begin_processing(&session_id).unwrap();
        assert_eq!(manager.phase(), Some(SessionPhase::Processing));
        assert_eq!(
            manager.begin_processing(&session_id),
            Err(SessionError::InvalidTransition {
                session_id,
                from: SessionPhase::Processing,
                to: SessionPhase::Processing,
            })
        );
    }

    #[test]
    fn cancel_cleans_overlay_and_owned_temp_files_and_returns_idle() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 1920, 1080).unwrap();
        let session_id = match manager.start(monitor, true) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };
        manager.begin_processing(&session_id).unwrap();
        manager
            .register_temp_file(&session_id, PathBuf::from("temporary/capture.png"))
            .unwrap();
        manager
            .mark_main_window_hidden(&session_id, MainWindowVisibilityRevision(7))
            .unwrap();

        let cleanup = manager.cancel(&session_id, MainWindowVisibilityRevision(7)).unwrap();
        assert_eq!(cleanup.session_id, session_id);
        assert_eq!(cleanup.temp_files, vec![PathBuf::from("temporary/capture.png")]);
        assert!(cleanup.hide_overlay);
        assert!(cleanup.restore_main_window);
        assert_eq!(manager.phase(), None);
        assert!(manager.current().is_none());
    }

    #[test]
    fn cleanup_does_not_restore_main_window_after_user_changed_visibility() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 1920, 1080).unwrap();
        let session_id = match manager.start(monitor, true) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };
        manager
            .mark_main_window_hidden(&session_id, MainWindowVisibilityRevision(10))
            .unwrap();

        let cleanup = manager.cancel(&session_id, MainWindowVisibilityRevision(11)).unwrap();
        assert!(!cleanup.restore_main_window);
        assert!(cleanup.hide_overlay);
        assert_eq!(manager.phase(), None);
    }

    #[test]
    fn stale_cancel_cannot_clean_the_current_session() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 1920, 1080).unwrap();
        let session_id = match manager.start(monitor, false) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };

        let error = manager.cancel("old-session", MainWindowVisibilityRevision(0));
        assert_eq!(
            error,
            Err(SessionError::StaleSession {
                expected: session_id,
                actual: "old-session".to_string(),
            })
        );
        assert_eq!(manager.phase(), Some(SessionPhase::Selecting));
    }

    #[test]
    fn finish_and_retain_file_removes_retained_file_before_taking_cleanup() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/screenshot/session.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取会话源码失败");
        let start = source.find("pub fn finish_and_retain_file").expect("缺少保留文件完成函数");
        let end = source[start..]
            .find("pub fn mark_main_window_hidden")
            .map(|offset| start + offset)
            .expect("缺少函数结束锚点");
        let body = &source[start..end];
        // 若顺序颠倒，take_cleanup 的清理计划会包含保留文件，cleanup_plan 将删除
        // 刚写入历史库的最终 PNG——顺序不变量必须锁定。
        let retain = body.find("session.temp_files.retain").expect("缺少保留文件剔除");
        let take = body.find("self.take_cleanup").expect("缺少清理计划获取");
        assert!(retain < take, "必须先剔除保留文件再取清理计划");
    }

    #[test]
    fn cancelled_processing_session_cannot_finish_a_new_session() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 1920, 1080).unwrap();
        let old_session_id = match manager.start(monitor, false) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };
        manager.begin_processing(&old_session_id).unwrap();
        manager
            .cancel(&old_session_id, MainWindowVisibilityRevision(0))
            .unwrap();

        let new_session_id = match manager.start(monitor, false) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("取消后应允许创建新会话，实际为 {other:?}"),
        };
        assert_ne!(old_session_id, new_session_id);

        assert!(matches!(
            manager.finish(&old_session_id, MainWindowVisibilityRevision(0)),
            Err(SessionError::StaleSession { .. })
        ));
        assert_eq!(manager.phase(), Some(SessionPhase::Selecting));
        assert_eq!(manager.current().unwrap().session_id(), new_session_id);
    }

    #[test]
    fn committing_session_rejects_late_cancel_and_finishes_cleanly() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 3840, 2160).unwrap();
        let session_id = match manager.start(monitor, false) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };

        manager.begin_processing(&session_id).unwrap();
        manager.begin_commit(&session_id).unwrap();
        assert_eq!(manager.phase(), Some(SessionPhase::Committing));
        assert_eq!(
            manager.cancel(&session_id, MainWindowVisibilityRevision(12)),
            Err(SessionError::CommitInProgress { session_id: session_id.clone() })
        );

        let cleanup = manager
            .finish(&session_id, MainWindowVisibilityRevision(12))
            .unwrap();

        assert_eq!(cleanup.session_id, session_id);
        assert!(cleanup.hide_overlay);
        assert_eq!(manager.phase(), None);
        assert!(manager.current().is_none());
    }

    #[test]
    fn committing_finish_retains_final_file_and_cleans_only_remaining_temp_files() {
        let mut manager = ScreenshotSessionManager::default();
        let monitor = MonitorRect::new(0, 0, 1920, 1080).unwrap();
        let session_id = match manager.start(monitor, true) {
            StartSessionResult::Started(session) => session.session_id().to_string(),
            other => panic!("首次启动应创建会话，实际为 {other:?}"),
        };
        manager.begin_processing(&session_id).unwrap();
        manager
            .register_temp_file(&session_id, PathBuf::from("temporary/encoded.png"))
            .unwrap();
        manager
            .register_temp_file(&session_id, PathBuf::from("temporary/capture.png"))
            .unwrap();
        manager
            .mark_main_window_hidden(&session_id, MainWindowVisibilityRevision(3))
            .unwrap();
        manager.begin_commit(&session_id).unwrap();

        let retained = PathBuf::from("temporary/encoded.png");
        let cleanup = manager
            .finish_and_retain_file(&session_id, MainWindowVisibilityRevision(3), &retained)
            .unwrap();

        assert_eq!(cleanup.session_id, session_id);
        // 成功保留的最终文件必须从清理清单剔除，其余会话临时文件仍应清理。
        assert_eq!(
            cleanup.temp_files,
            vec![PathBuf::from("temporary/capture.png")]
        );
        assert!(!cleanup.temp_files.contains(&retained));
        assert!(cleanup.hide_overlay);
        assert!(cleanup.restore_main_window);
        assert_eq!(manager.phase(), None);
        assert!(manager.current().is_none());
    }
}
