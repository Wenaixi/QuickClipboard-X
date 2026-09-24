//! 统一事件总线：core 侧生产、gui 侧消费。
//!
//! eframe 没有 AppHandle/emit，所有跨线程通知改为「无界 mpsc 队列 +
//! 全局重绘钩子」：任意线程 `post` 投递并触发一次重绘，UI 线程每帧
//! `drain` 排空。core 不依赖 egui——重绘钩子由 gui 在启动时注册。

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{LazyLock, Mutex};

use crate::services::database::ClipboardItem;

/// 剪贴板更新事件的载荷（字段与旧 ClipboardUpdatedEventPayload 一致）
#[derive(Debug, Clone)]
pub struct ClipboardUpdatedEvent {
    pub kind: String,
    pub item: Option<ClipboardItem>,
    pub insert_index: Option<i64>,
    pub total_count: Option<i64>,
}

/// core 全域通知事件
#[derive(Debug, Clone)]
pub enum AppEvent {
    ClipboardUpdated(ClipboardUpdatedEvent),
    PasteCountUpdated(i64),
    FavoritePasteCountUpdated(String),
    /// 低占用模式下的托盘菜单需要重建
    TrayMenuRefresh,
}

/// 全局生产端与消费端。
/// 使用无界 channel：`send` 不会失败也不会丢弃事件——丢一条剪贴板更新
/// 就等于 UI 少一条记录，宁可增长队列也不能静默丢。
static EVENTS: LazyLock<(Sender<AppEvent>, Mutex<Receiver<AppEvent>>)> =
    LazyLock::new(|| {
        let (tx, rx) = channel();
        (tx, Mutex::new(rx))
    });

/// 重绘钩子：gui 注册为 `ctx.request_repaint()`，core 不感知 egui。
static REPAINT: LazyLock<Mutex<Option<Box<dyn Fn() + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(None));

/// 注册重绘钩子（gui 启动时调用一次）
pub fn set_repaint_hook(hook: Box<dyn Fn() + Send + Sync>) {
    *REPAINT.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}

/// 生产端：任意线程可调，投递后请求一次重绘
pub fn post(event: AppEvent) {
    let (tx, _) = &*EVENTS;
    let _ = tx.send(event);
    if let Some(hook) = REPAINT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        hook();
    }
}

/// UI 线程消费：每帧排空队列
pub fn drain() -> Vec<AppEvent> {
    let (_, rx) = &*EVENTS;
    let rx = rx.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(event);
    }
    out
}

/// 保留「主窗口不可见 → 只打 pending 标记、不发事件」的既有语义
pub fn emit_clipboard_updated(event: ClipboardUpdatedEvent) {
    if !crate::windows::main_window::is_main_window_visible_for_updates() {
        crate::windows::main_window::mark_clipboard_refresh_pending();
        return;
    }
    post(AppEvent::ClipboardUpdated(event));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // 事件总线是进程级全局单队列,两个测试并发跑会互相 drain 清空对方
    // 投递的事件(§10.4 共享全局静态并发污染)。模块内串行化。
    static SERIAL: Mutex<()> = Mutex::new(());

    fn lock_serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    // 事件总线必须能跨线程投递并按 FIFO 排空。
    // 注意:队列是进程级全局,其它测试(尤其 clipboard 相关)可能在同一
    // 进程内投递过事件,不能假设 drained 只含本测试投递的两条——只断言
    // 本测试投递的两条必定按序出现即可。
    #[test]
    fn events_round_trip_in_order() {
        let _g = lock_serial();
        post(AppEvent::PasteCountUpdated(1));
        post(AppEvent::FavoritePasteCountUpdated("fav".into()));
        let drained = drain();
        assert!(drained.len() >= 2, "投递的事件必须能被排空");
        let p1 = drained
            .iter()
            .position(|e| matches!(e, AppEvent::PasteCountUpdated(1)))
            .expect("必须能找到 PasteCountUpdated(1)");
        let fav = drained
            .iter()
            .position(|e| matches!(e, AppEvent::FavoritePasteCountUpdated(_)))
            .expect("必须能找到 FavoritePasteCountUpdated");
        assert!(p1 < fav, "FIFO:先投递的 PasteCountUpdated(1) 必须先被排空");
    }

    // drain 之后队列必须为空（不重复消费）
    #[test]
    fn drain_empties_queue() {
        let _g = lock_serial();
        post(AppEvent::TrayMenuRefresh);
        let _ = drain();
        assert!(drain().is_empty(), "排空后再次 drain 必须为空");
    }
}
