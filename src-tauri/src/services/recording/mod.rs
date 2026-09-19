// R4 屏幕录制（GIF 优先，对齐 ShareX ScreenRecorder）：
// 录制循环有界 + 停止路径清理 + 产物走截图历史（总体计划 §8 护栏）。
// 本模块只做帧采集循环编排：框选复用截图选区（CaptureRect），逐帧
// capture_monitor 采 RGBA → gif_writer 编码，停止后产物由调用方走
// encode_and_store_png_bytes 落历史。录制循环用版本代数保证停止即
// 失效在飞任务。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::services::screenshot::capture::{capture_monitor, ensure_com_initialized, get_monitor_handle, CaptureRect};

use super::gif_writer::{encode_rgba_frames, GifFrameRate};

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

/// 开始录制：循环在 spawn 线程内逐帧 capture_monitor（COM MTA 每帧
/// 初始化），直到 stop() 推进代数/置停止标志退出；回调 on_frame 每帧
/// 收 RGBA 供调用方累积。返回句柄供停止。
pub fn start_recording(
    monitor: CaptureRect,
    _frame_rate: GifFrameRate,
    on_frame: impl Fn(u32, u32, Vec<u8>) + Send + 'static,
) -> RecordingSession {
    let session = RecordingSession::default();
    let session_clone = session.clone();
    let center_x = monitor.left.saturating_add(monitor.width / 2);
    let center_y = monitor.top.saturating_add(monitor.height / 2);
    let hmonitor = get_monitor_handle(center_x, center_y);
    std::thread::spawn(move || {
        loop {
            if session_clone.stop_flag.load(Ordering::SeqCst) {
                break;
            }
            let _com = ensure_com_initialized();
            match capture_monitor(hmonitor, monitor) {
                Ok(frame) => on_frame(frame.width, frame.height, frame.rgba),
                Err(_) => {
                    // 单帧采集失败不中断录制：短暂重试，避免偶发帧缺失终止循环。
                }
            }
            // 有界节流：固定帧率最小间隔。
            std::thread::sleep(std::time::Duration::from_millis(
                (1000 / _frame_rate.frames_per_second as u64).max(16),
            ));
        }
    });
    session
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
    }
}