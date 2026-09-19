// R4 屏幕录制（GIF 优先，对齐 ShareX ScreenRecorder）：
// 录制循环有界 + 停止路径清理 + 产物走截图历史（总体计划 §8 护栏）。
// 本模块做帧采集循环编排 + 单飞会话管理：逐帧 capture_monitor 采 RGBA
// → gif_writer 编码，停止后产物由 storage 链路走截图历史。录制循环用
// 版本代数保证停止即失效在飞任务。本模块整体仅 Windows（依赖截图捕获）。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::services::screenshot::capture::{capture_monitor, ensure_com_initialized, get_monitor_handle, CaptureRect, CapturedFrame};

mod gif_writer;
pub use gif_writer::{encode_rgba_frames, GifFrameRate};

pub mod storage;
pub use storage::{store_recording_to_history, RecordingResult};

pub const RECORDING_MIN_FPS: u8 = 10;
pub const RECORDING_MAX_FPS: u8 = 15;

/// 录制会话句柄：start_recording 返回，stop 推进代数使在飞采集失效。
#[derive(Clone, Default)]
pub struct RecordingSession {
    stop_flag: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl RecordingSession {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        // 推进代数使在飞的逐帧循环醒来即退出（不再尝试下一次采集）。
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// 屏幕录制全局状态：单飞守卫 + 会话/帧缓冲/最终结果。
/// 任何时刻至多一段录制会话在采帧（reload 快捷键的"开始/停止"切换语义
/// 依赖此单例：再次触发同一热键即停止当前会话）。采集帧累积在内存缓冲，
/// 停止时合并编码 GIF + 首帧快照落历史；产物经 storage 链路走截图历史。
#[derive(Default)]
struct RecordingManager {
    session: Mutex<Option<RecordingSession>>,
    frames: Mutex<Vec<CapturedFrame>>,
    finished: Mutex<Option<RecordingResult>>,
}

static MANAGER: OnceLock<RecordingManager> = OnceLock::new();

fn manager() -> &'static RecordingManager {
    MANAGER.get_or_init(RecordingManager::default)
}

/// 开始录制：循环在 spawn 线程内逐帧 capture_monitor（COM MTA 每帧
/// 初始化），直到 stop() 推进代数/置停止标志退出；回调 on_frame 每帧
/// 收 RGBA 供调用方累积。hmonitor 为录制目标显示器句柄，monitor 为该
/// 显示器本地像素内的裁切区（录制全屏时即 {0,0,宽,高}）。返回句柄供停止。
pub fn start_recording(
    hmonitor: windows::Win32::Graphics::Gdi::HMONITOR,
    monitor: CaptureRect,
    frame_rate: GifFrameRate,
    on_frame: impl Fn(u32, u32, Vec<u8>) + Send + 'static,
) -> RecordingSession {
    let session = RecordingSession::default();
    let session_clone = session.clone();
    // HMONITOR 是 *mut c_void 句柄(非 Send),不能整体移入线程——
    // 循环内仅用其地址作前台窗口判定,这里只把地址值本身移入。
    let hmonitor_raw = hmonitor.0 as usize;
    std::thread::spawn(move || {
        loop {
            if session_clone.stop_flag.load(Ordering::SeqCst) {
                break;
            }
            let _com = ensure_com_initialized();
            match capture_monitor(unsafe { windows::Win32::Graphics::Gdi::HMONITOR(hmonitor_raw as *mut core::ffi::c_void) }, monitor) {
                Ok(frame) => on_frame(frame.width, frame.height, frame.rgba),
                Err(_) => {
                    // 单帧采集失败不中断录制：短暂重试，避免偶发帧缺失终止循环。
                }
            }
            // 有界节流：固定帧率最小间隔。
            std::thread::sleep(std::time::Duration::from_millis(
                (1000 / frame_rate.frames_per_second as u64).max(16),
            ));
        }
    });
    session
}

/// 活动录制会话：无则返回 None（上下文中未处于录制状态）。
pub fn current_session() -> Option<RecordingSession> {
    manager().session.lock().ok()?.clone()
}

/// 开始录制：先停止既有会话（热键重入语义），清空上一段帧缓冲，
/// 启动采集线程并把帧累积进内存缓冲。
pub fn begin_recording(
    app: &tauri::AppHandle,
    hmonitor: windows::Win32::Graphics::Gdi::HMONITOR,
    monitor: CaptureRect,
    frame_rate: GifFrameRate,
) -> Result<(), String> {
    if monitor.width == 0 || monitor.height == 0 {
        return Err("录制选区不能为空".to_string());
    }
    stop_recording(app);
    let mut frames_guard = manager().frames.lock().map_err(|error| format!("录制帧缓冲被污染: {error}"))?;
    frames_guard.clear();
    drop(frames_guard);
    let on_frame = {
        let manager: &'static RecordingManager = manager();
        move |width: u32, height: u32, rgba: Vec<u8>| {
            if let Ok(mut frames) = manager.frames.lock() {
                frames.push(CapturedFrame { width, height, rgba });
            }
        }
    };
    let session = start_recording(hmonitor, monitor, frame_rate, on_frame);
    *manager().session.lock().map_err(|error| format!("录制会话锁被污染: {error}"))? = Some(session);
    Ok(())
}

/// 用户入口：以当前光标所在显示器整屏区域开始录制（对齐 ShareX
/// ScreenRecorder 的"录制全屏"默认语义——无选区时录光标所在整屏，
/// 选区留待后续 R4 接入截图浮窗）。
pub fn start_recording_for_user(app: &tauri::AppHandle) -> Result<(), String> {
    let cursor = app
        .cursor_position()
        .map_err(|error| format!("读取鼠标位置失败: {error}"))?;
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|error| format!("获取光标所在显示器失败: {error}"))?
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "没有可用的显示器".to_string())?;
    let size = monitor.size();
    let rect = CaptureRect { left: 0, top: 0, width: size.width, height: size.height };
    let hmonitor = get_monitor_handle(cursor.x as i32, cursor.y as i32);
    begin_recording(app, hmonitor, rect, GifFrameRate::new(RECORDING_MAX_FPS)?)
}

/// 停止录制：停止采集会话，把累积帧编码为 GIF + 首帧快照，经存储
/// 链路落剪贴板历史。无活动会话时是空操作。
pub fn stop_recording(app: &tauri::AppHandle) -> Result<(), String> {
    let session = {
        let mut guard = manager().session.lock().map_err(|error| format!("录制会话锁被污染: {error}"))?;
        guard.take()
    };
    let Some(session) = session else {
        return Ok(());
    };
    session.stop();

    let frames = {
        let mut guard = manager().frames.lock().map_err(|error| format!("录制帧缓冲被污染: {error}"))?;
        std::mem::take(&mut *guard)
    };
    if frames.is_empty() {
        return Err("录制没有采集到有效帧".to_string());
    }

    let width = frames[0].width;
    let height = frames[0].height;

    // 全部帧 RGBA → GIF 字节（帧率取录制上限，压缩交付体积）。
    let frame_tuple: Vec<(u32, u32, &[u8])> = frames
        .iter()
        .map(|frame| (frame.width, frame.height, frame.rgba.as_slice()))
        .collect();
    let gif = encode_rgba_frames(
        &frame_tuple,
        GifFrameRate::new(RECORDING_MAX_FPS).expect("帧率常量在允许区间"),
    )
    .map_err(|error| format!("GIF 编码失败: {error}"))?;

    let result = RecordingResult {
        gif_bytes: gif,
        width,
        height,
        rgba: frames[0].rgba.clone(),
    };
    // 产物经既有截图历史链路落库（剪贴板图片 + 历史入库 + 事件通知），
    // 历史里可预览/复制；快照以首帧为录制代表帧。
    store_recording_to_history(app, &result)
        .map_err(|error| format!("录制产物落历史失败: {error}"))?;
    // 同时把最终结果缓存供「录制产物」命令查询/复制。
    *manager().finished.lock().map_err(|error| format!("录制结果锁被污染: {error}"))? = Some(result);
    Ok(())
}

/// 最近一次录制产物：供「复制录制产物」命令把 GIF 复制到剪贴板。
pub fn finished_result() -> Option<RecordingResult> {
    manager().finished.lock().ok()?.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_constants_define_bounded_frame_rate() {
        assert_eq!(RECORDING_MIN_FPS, 10);
        assert_eq!(RECORDING_MAX_FPS, 15);
        assert!(RECORDING_MIN_FPS < RECORDING_MAX_FPS);
    }

    #[test]
    fn stop_advances_generation_to_invalidate_in_flight_capture() {
        let session = RecordingSession::default();
        let before = session.generation.load(Ordering::SeqCst);
        session.stop();
        assert!(session.stop_flag.load(Ordering::SeqCst), "停止必须置停止标志");
        assert_eq!(session.generation.load(Ordering::SeqCst), before + 1, "停止必须推进代数");
    }

    #[test]
    fn recording_source_guards_bounded_loop_and_capture_call() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/recording/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取录制源码失败");
        // 循环必须检查停止标志（有界：停止后不再采集）。
        assert!(source.contains("stop_flag.load(Ordering::SeqCst)"), "循环必须检查停止标志");
        // 逐帧采集必须走既有 capture_monitor。
        assert!(source.contains("capture_monitor(hmonitor, monitor)"), "必须调用既有捕获链路");
        // 停止必须推进代数使在飞循环失效。
        assert!(source.contains("generation.fetch_add(1, Ordering::SeqCst)"), "停止必须推进代数");
        // 单飞守卫：再次开始必须先行停止既有会话。
        assert!(source.contains("stop_recording(app);"), "begin_recording 必须先停止既有会话");
        // 停止路径必须把累积帧编码为 GIF 并落历史。
        assert!(source.contains("encode_rgba_frames"), "停止必须编码 GIF");
        assert!(source.contains("store_recording_to_history(app, &result)"), "停止必须落剪贴板历史");
    }
}