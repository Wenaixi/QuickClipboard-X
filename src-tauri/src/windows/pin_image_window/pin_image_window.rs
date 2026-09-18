use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::OnceCell;
use std::time::{Duration, Instant};
use tauri::{AppHandle, WebviewWindow};

static PIN_IMAGE_COUNTER: AtomicUsize = AtomicUsize::new(0);
static PIN_IMAGE_DATA_MAP: OnceCell<Mutex<HashMap<String, PinImageData>>> = OnceCell::new();

// 预览窗口建窗流程串行化锁——固定标签 "image-preview" 下并发请求会交错
// close/insert/create,数据与窗口错位(详见 pin_image_from_file 注释)。tokio
// 锁可跨 await 持有,命令 future 保持 Send;OnceCell 惰性初始化同 PIN_IMAGE_DATA_MAP。
static PREVIEW_WINDOW_LOCK: OnceCell<tokio::sync::Mutex<()>> = OnceCell::new();

// 锁 helper:OnceCell + Mutex poison 双重处理统一恢复语义
fn lock_pin_data() -> std::sync::MutexGuard<'static, HashMap<String, PinImageData>> {
    PIN_IMAGE_DATA_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

const DEFAULT_PREVIEW_SIZE: u32 = 600;

#[derive(Clone, Debug)]
struct PinImageData {
    file_path: String,
    width: u32,
    height: u32,
    preview_mode: bool,
    image_physical_x: Option<i32>,
    image_physical_y: Option<i32>,
    original_image_path: Option<String>, 
    edit_data: Option<String>,           
}

pub fn init_pin_image_window() {
    PIN_IMAGE_COUNTER.store(0, Ordering::SeqCst);
    PIN_IMAGE_DATA_MAP.get_or_init(|| Mutex::new(HashMap::new()));
    // 登记 AppHandle 供 GDI 菜单 SaveAs 使用(setup 阶段调用一次)
    if let Some(app) = crate::services::store::app_handle_raw() {
        super::gdi::init_app_handle(app);
    }
    // 加载贴图设置文件并注入 GDI 默认状态(新窗口按此应用偏好)
    super::gdi_settings::init_pin_image_settings();
}


// 从文件路径创建贴图窗口
#[tauri::command]
pub async fn pin_image_from_file(
    app: AppHandle,
    file_path: String,
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    preview_mode: Option<bool>,
    image_physical_x: Option<i32>,
    image_physical_y: Option<i32>,
    image_physical_width: Option<u32>,
    image_physical_height: Option<u32>,
    original_image_path: Option<String>,
    edit_data: Option<String>,
) -> Result<(), String> {
    let is_preview = preview_mode.unwrap_or(false);
    let use_physical_coords = image_physical_x.is_some() && image_physical_y.is_some();
    
    let (img_width, img_height, pos_x, pos_y) = if is_preview {
        let (orig_w, orig_h) = read_image_logical_size(&file_path, &app)?;
        let (img_w, img_h) = scale_for_preview(orig_w, orig_h, &app);

        let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
        let (mon_x, mon_y, mon_right, mon_bottom, scale_factor) = crate::utils::screen::ScreenUtils::get_monitor_at_cursor(&app)
            .map(|m| {
                let pos = m.position();
                let size = m.size();
                (pos.x, pos.y, pos.x + size.width as i32, pos.y + size.height as i32, m.scale_factor())
            })
            .unwrap_or((0, 0, 1920, 1080, 1.0));

        let window_w = ((img_w as f64 + 10.0) * scale_factor).round() as i32;
        let window_h = ((img_h as f64 + 10.0) * scale_factor).round() as i32;
        let px = if mon_right - cursor_x >= window_w { cursor_x } else { cursor_x - window_w };
        let py = if mon_bottom - cursor_y >= window_h { cursor_y } else { cursor_y - window_h };

        (img_w, img_h, px.max(mon_x), py.max(mon_y))
    } else if use_physical_coords {
        let img_x = image_physical_x.unwrap();
        let img_y = image_physical_y.unwrap();
        let img_phys_w = image_physical_width.unwrap_or(100);
        let img_phys_h = image_physical_height.unwrap_or(100);

        let scale_factor = crate::utils::screen::ScreenUtils::get_scale_factor_at_point(&app, img_x, img_y);
        let padding = (5.0 * scale_factor).round() as i32;
        let logical_w = (img_phys_w as f64 / scale_factor).round() as u32;
        let logical_h = (img_phys_h as f64 / scale_factor).round() as u32;

        (logical_w.max(1), logical_h.max(1), img_x - padding, img_y - padding)
    } else if let (Some(px), Some(py)) = (x, y) {
        // x/y 分支窗口落在指定坐标所在显示器——图片逻辑尺寸必须按该屏
        // scale 折算,否则两屏 scale 不同时 inner_size(logical) 与 set_position
        // (physical) 混用导致物理尺寸错误。
        let (w, h) = if let (Some(w), Some(h)) = (width, height) {
            (w, h)
        } else {
            read_image_logical_size_at(&file_path, &app, px, py)?
        };
        (w, h, px, py)
    } else {
        let (w, h) = if let (Some(w), Some(h)) = (width, height) {
            (w, h)
        } else {
            read_image_logical_size(&file_path, &app)?
        };
        let (cx, cy) = center_position(&app, w, h);
        (w, h, cx, cy)
    };
    
    // 生成窗口标签
    let window_label = if is_preview {
        "image-preview".to_string()
    } else {
        format!("pin-image-{}", PIN_IMAGE_COUNTER.fetch_add(1, Ordering::SeqCst))
    };

    // 预览窗口整个建窗流程串行化(建窗前就取锁)——固定标签 "image-preview"
    // 下两个并发请求(如预览与真实贴图交错、或 menu hover 连续触发)会交错执行:
    // A 关闭旧窗、A 写入数据、B 关闭 A 新窗、A/B 各自 create 同一标签(第二个
    // WebviewWindowBuilder::build 对已存在 label 抛错或复用),窗口与其数据错位。
    // 注:文字描述历史实现——GDI 版已不用 WebviewWindowBuilder,此注释保留背景。
    // 非预览窗口标签唯一(PIN_IMAGE_COUNTER 递增),无此竞态,不需要取锁。
    let _preview_guard = if is_preview {
        Some(PREVIEW_WINDOW_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await)
    } else {
        None
    };

    if is_preview {
        // 预览模式固定标签:建窗前先清理旧窗口与旧数据(与 WebView 版
        // close 旧窗语义一致)。此处已持有 PREVIEW_WINDOW_LOCK,不能调
        // close_image_preview(它会 blocking_lock 同一把锁造成死锁),
        // 直接移除数据 + PostMessage(WM_CLOSE) 即可。
        if super::gdi::find_gdi_window(&window_label).is_some() {
            lock_pin_data().remove(&window_label);
            if let Some(hwnd) = super::gdi::find_gdi_window(&window_label) {
                let _ = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    )
                };
            }
        }
    }
    
    // 存储图片数据
    let actual_original_path = original_image_path.or_else(|| Some(file_path.clone()));
    lock_pin_data().insert(
        window_label.clone(),
        PinImageData {
            file_path,
            width: img_width,
            height: img_height,
            preview_mode: is_preview,
            image_physical_x,
            image_physical_y,
            original_image_path: actual_original_path,
            edit_data,
        },
    );

    create_pin_image_window(&app, &window_label, img_width, img_height, pos_x, pos_y).await?;
    Ok(())
}

// 读取图片逻辑尺寸
fn read_image_logical_size(file_path: &str, app: &AppHandle) -> Result<(u32, u32), String> {
    let reader = image::ImageReader::open(file_path)
        .map_err(|e| format!("打开图片文件失败: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("识别图片格式失败: {}", e))?;

    let (w, h) = reader.into_dimensions()
        .map_err(|e| format!("读取图片尺寸失败: {}", e))?;

    let scale_factor = crate::utils::screen::ScreenUtils::get_monitor_at_cursor(app)
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    Ok(((w as f64 / scale_factor).round() as u32, (h as f64 / scale_factor).round() as u32))
}

// 按目标坐标所在显示器折算图片逻辑尺寸——贴图窗口用 inner_size(logical)
// 建窗、set_position(physical) 落位,若目标屏与光标屏 scale 不同,仍按光标屏
// 折算会让物理尺寸错误(如 100% 屏折出的 1.2x 逻辑尺寸落在 200% 屏上被放大
// 一倍)。use_physical_coords 分支已用 get_scale_factor_at_point,此变体与之对齐。
fn read_image_logical_size_at(
    file_path: &str,
    app: &AppHandle,
    target_x: i32,
    target_y: i32,
) -> Result<(u32, u32), String> {
    let reader = image::ImageReader::open(file_path)
        .map_err(|e| format!("打开图片文件失败: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("识别图片格式失败: {}", e))?;

    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| format!("读取图片尺寸失败: {}", e))?;

    let scale_factor =
        crate::utils::screen::ScreenUtils::get_scale_factor_at_point(app, target_x, target_y);

    Ok(((w as f64 / scale_factor).round() as u32, (h as f64 / scale_factor).round() as u32))
}

// 预览模式缩放
fn scale_for_preview(width: u32, height: u32, app: &AppHandle) -> (u32, u32) {
    let preview_size = crate::utils::screen::ScreenUtils::get_monitor_at_cursor(app)
        .map(|m| {
            let size = m.size();
            let sf = m.scale_factor();
            ((size.height as f64 / sf / 2.0) as u32).min(DEFAULT_PREVIEW_SIZE)
        })
        .unwrap_or(DEFAULT_PREVIEW_SIZE);
    
    let max_side = width.max(height);
    if max_side == 0 {
        (preview_size, preview_size)
    } else {
        let scale = preview_size as f64 / max_side as f64;
        (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1))
    }
}

// 计算屏幕中心位置
fn center_position(app: &AppHandle, width: u32, height: u32) -> (i32, i32) {
    if let Ok(monitor) = crate::utils::screen::ScreenUtils::get_monitor_at_cursor(app) {
        let pos = monitor.position();
        let size = monitor.size();
        let sf = monitor.scale_factor();
        let win_w = ((width as f64 + 10.0) * sf).round() as i32;
        let win_h = ((height as f64 + 10.0) * sf).round() as i32;
        (pos.x + (size.width as i32 - win_w) / 2, pos.y + (size.height as i32 - win_h) / 2)
    } else {
        (100, 100)
    }
}


// 创建贴图窗口
async fn create_pin_image_window(
    app: &AppHandle,
    label: &str,
    width: u32,
    height: u32,
    physical_x: i32,
    physical_y: i32,
) -> Result<(), String> {
    // 走 GDI 分层窗口:原 WebView 版 inner_size(logical)+set_position(physical)
    // 混合折算,GDI 版直接按目标屏 scale 折算物理尺寸建窗,逻辑一致。
    let scale = crate::utils::screen::ScreenUtils::get_scale_factor_at_point(app, physical_x, physical_y);
    let (physical_w, physical_h) = (
        ((width as f64 + 10.0) * scale).round() as u32,
        ((height as f64 + 10.0) * scale).round() as u32,
    );
    let image_path = {
        let map = lock_pin_data();
        map.get(label).map(|d| d.file_path.clone())
    };
    let hwnd = super::gdi::create_gdi_window(label, physical_x, physical_y, physical_w, physical_h, is_preview_label(label))?;
    if let Some(path) = image_path {
        super::gdi::render_image(hwnd, &path, 255)?;
    }
    Ok(())
}

/// 预览窗口固定标签
fn is_preview_label(label: &str) -> bool {
    label == "image-preview"
}

// 图片数据查询:由 GDI 菜单/另存/清理按标签读 PIN_IMAGE_DATA_MAP。
// 原命令壳 get_pin_image_data(WebviewWindow 版)随前端删除后无调用者,
// 按 R37 同标准删除命令注册,服务逻辑保留在 close/save 内部。
fn pin_image_data(label: &str) -> Result<serde_json::Value, String> {
    let map = lock_pin_data();
    if let Some(data) = map.get(label) {
        return Ok(json!({
            "file_path": data.file_path,
            "width": data.width,
            "height": data.height,
            "preview_mode": data.preview_mode,
            "image_physical_x": data.image_physical_x,
            "image_physical_y": data.image_physical_y,
            "original_image_path": data.original_image_path,
            "edit_data": data.edit_data
        }));
    }
    Err("未找到图片数据".to_string())
}

// 按标签取贴图文件路径(菜单复制/另存/透明度重渲染共用)
pub fn pin_image_file_path(label: &str) -> Result<String, String> {
    let map = lock_pin_data();
    map.get(label)
        .map(|d| d.file_path.clone())
        .ok_or_else(|| "未找到图片数据".to_string())
}

// 图片另存为:由 GDI 右键菜单按 label 调用(命令壳已删,保留服务函数)
pub async fn save_pin_image_as(app: AppHandle, label: String) -> Result<(), String> {
    use std::path::Path;
    use tauri_plugin_dialog::DialogExt;

    let file_path = {
        let map = lock_pin_data();
        if let Some(data) = map.get(&label) {
            data.file_path.clone()
        } else {
            return Err("未找到图片数据".to_string());
        }
    };

    let path = Path::new(&file_path);
    if !path.exists() {
        return Err("图片文件不存在".to_string());
    }

    let filename = format!("QC_{}.png",
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
    );

    let save_path = app.dialog().file()
        .set_file_name(&filename)
        .add_filter("PNG 图片", &["png"])
        .add_filter("JPEG 图片", &["jpg", "jpeg"])
        .add_filter("所有文件", &["*"])
        .blocking_save_file()
        .ok_or("用户取消保存")?;

    let dest = save_path.as_path().ok_or("无效的文件路径")?;
    std::fs::copy(&file_path, dest)
        .map_err(|e| format!("保存失败: {}", e))?;

    Ok(())
}

// 关闭预览窗口
// close/remove 同样纳入 PREVIEW_WINDOW_LOCK 串行化——否则建窗
// 流程(pin_image_from_file 预览分支)await create_pin_image_window 期间,并发的
// close_image_preview 可移除刚 insert 的 PinImageData 并关旧窗,建窗完成新窗
// 渲染后其数据读 map 为空返回 Err,预览窗空屏+穿透残留。
// 本函数是同步 fn,取锁必须用 blocking_lock()——tokio::sync::Mutex
// 的 .lock() 返回一个 future,在同步 fn 里从不被 poll,锁实际从未获取,串行化
// 形同虚设。blocking_lock 是 tokio Mutex 提供的同步获取方式,与 pin_image_from_file
// 预览分支的 .lock().await 共用同一把锁,阻塞持有时间极短,不会拖慢建窗流程。
#[tauri::command]
pub fn close_image_preview(app: AppHandle) -> Result<(), String> {
    let label = "image-preview";
    let _preview_guard = if super::gdi::find_gdi_window(label).is_some() {
        Some(PREVIEW_WINDOW_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).blocking_lock())
    } else {
        None
    };
    lock_pin_data().remove(label);
    if let Some(hwnd) = super::gdi::find_gdi_window(label) {
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            )
        };
    }
    Ok(())
}

// 按标签清理贴图数据:移除 PIN_IMAGE_DATA_MAP 记录,并在文件路径与原始
// 图片不同(独立临时文件)且不再被其他贴图引用时删除临时文件。前端主动
// 关窗与低占用模式销毁贴图窗口共用同一清理语义,保证两条销毁路径一致,
// 避免直接 destroy 绕过清理造成数据残留与临时文件泄漏。
pub fn cleanup_pin_image_data(label: &str) {
    let mut map = lock_pin_data();
    if let Some(data) = map.remove(label) {
        if let Some(ref original_path) = data.original_image_path {
            if data.file_path != *original_path {
                let file_in_use = map.values().any(|d| d.file_path == data.file_path);
                if !file_in_use {
                    let _ = std::fs::remove_file(&data.file_path);
                }
            }
        }
    }
}

// 全部贴图归零清理:低占用模式销毁全部贴图窗口前调用,逐个走
// cleanup_pin_image_data 的清理语义——移除数据记录、删除不再被引用的
// 独立临时文件;map 已空时锁被其他路径持有也无妨,remove 无对象即跳过。
pub fn cleanup_all_pin_images() {
    for label in lock_pin_data().keys().cloned().collect::<Vec<String>>() {
        cleanup_pin_image_data(&label);
    }
}

// 关闭贴图窗口
/// 关闭贴图窗口:由 GDI 双键/菜单"关闭"直接调用(命令壳已删,服务函数保留)。
/// 走 cleanup_pin_image_data 统一清理语义(移除数据 + 删除不再被引用的临时文件),
/// 然后通知 GDI 层关闭窗口(WM_CLOSE → WM_DESTROY 清理 HWND 表)。
pub fn close_pin_image_window(label: &str) -> Result<(), String> {
    cleanup_pin_image_data(label);
    if let Some(hwnd) = super::gdi::find_gdi_window(label) {
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            )
        };
    }
    Ok(())
}

/// 启动贴图编辑模式
#[tauri::command]
pub async fn start_pin_edit_mode(
    _app: AppHandle,
    _window: WebviewWindow,
    _img_offset_x_physical: i32,
    _img_offset_y_physical: i32,
    _img_width_physical: u32,
    _img_height_physical: u32,
) -> Result<(), String> {
    // 贴图编辑功能当前未实现——旧实现是 1s 阻塞桩:注册一次性的
    // "pin-edit-ready" 监听 + rx.recv_timeout(1s) 阻塞等待,超时后返回
    // Err,还在 app.once 里残留监听(窗口被前端发射事件时会执行 hide+缩到
    // 1x1 的副作用)。直接返回不可用,不注册监听、不阻塞、不留挂起状态。
    Err("贴图编辑功能当前不可用".to_string())
}

/// 窗口缩放动画:由 GDI 缩略图切换/窗口缩放调用(命令壳已删,转内部函数)。
/// 复用原缓动公式 1 - 2^(-10t),逐帧 SetWindowPos(SWP_NOACTIVATE 防抢焦点)。
pub fn animate_window_resize(
    label: String,
    start_w: f64, start_h: f64,
    start_x: i32, start_y: i32,
    end_w: f64, end_h: f64,
    end_x: i32, end_y: i32,
    duration_ms: u64,
) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        let start_time = Instant::now();
        let duration = Duration::from_millis(duration_ms);
        let frame_duration = Duration::from_millis(16);
        let mut current_w = start_w as u32;
        let mut current_h = start_h as u32;
        let mut current_x = start_x as i32;
        let mut current_y = start_y as i32;

        let dw = end_w - start_w;
        let dh = end_h - start_h;
        let dx = end_x - start_x;
        let dy = end_y - start_y;

        loop {
            let elapsed = start_time.elapsed();
            if elapsed >= duration {
                let _ = set_gdi_window_geometry(&label, end_w as u32, end_h as u32, end_x, end_y);
                break;
            }

            let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
            let eased = 1.0 - 2f64.powf(-10.0 * progress);

            current_w = (start_w + dw * eased).round() as u32;
            current_h = (start_h + dh * eased).round() as u32;
            current_x = start_x + (dx as f64 * eased).round() as i32;
            current_y = start_y + (dy as f64 * eased).round() as i32;

            let _ = set_gdi_window_geometry(&label, current_w, current_h, current_x, current_y);

            tokio::time::sleep(frame_duration).await;
        }
    });

    Ok(())
}

/// 设置 GDI 窗口几何(尺寸+位置):内部 helper,动画与菜单共用
fn set_gdi_window_geometry(label: &str, w: u32, h: u32, x: i32, y: i32) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSENDCHANGING};
    let Some(hwnd) = super::gdi::find_gdi_window(label) else {
        return Err("贴图窗口不存在".to_string());
    };
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            w as i32,
            h as i32,
            SWP_NOACTIVATE | SWP_NOSENDCHANGING,
        )
    }
    .map_err(|e| format!("调整贴图窗口失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // §11.2:全局静态测试串行化(同 §7.12 state.rs SERIAL 模式)
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    // 贴图 scale 用错屏:x/y 与 center 分支的图片逻辑尺寸必须按目标
    // 坐标所在显示器 scale 折算(get_scale_factor_at_point),不能用光标屏
    // scale——两屏 scale 不同时 inner_size(logical)+set_position(physical)
    // 混用导致物理尺寸错误。preview 分支在光标屏布局,继续用光标屏 scale。
    #[test]
    fn x_y_branch_reads_logical_size_by_target_monitor_scale() {
        let src = std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/pin_image_window.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图窗口源码失败");
        let stripped: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub async fn pin_image_from_file")
            .expect("缺 pin_image_from_file");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        // x/y 分支(非 preview、非 physical coords)必须走按目标坐标折算的变体
        let x_y_pos = body
            .find("else if let (Some(px), Some(py)) = (x, y)")
            .expect("缺 x/y 分支");
        let x_y_seg = &body[x_y_pos..];
        assert!(
            x_y_seg.contains("read_image_logical_size_at(&file_path, &app, px, py)"),
            "x/y 分支必须按目标坐标所在显示器 scale 折算逻辑尺寸"
        );
        // 变体函数体内必须用 get_scale_factor_at_point(目标屏),而非光标屏
        let fn_pos = stripped
            .find("fn read_image_logical_size_at")
            .expect("缺 read_image_logical_size_at");
        let fn_rest = &stripped[fn_pos..];
        let fn_end = fn_rest.find("\n}\n").map(|i| fn_pos + i).unwrap_or(stripped.len());
        let fn_body = &stripped[fn_pos..fn_end];
        assert!(
            fn_body.contains("get_scale_factor_at_point(app, target_x, target_y)"),
            "read_image_logical_size_at 必须按目标坐标所在屏 scale 折算"
        );
        assert!(
            !fn_body.contains("get_monitor_at_cursor"),
            "read_image_logical_size_at 不得用光标屏 scale"
        );
    }

    #[test]
    fn pin_image_update_functions_are_not_reintroduced() {
        // 死功能链：后端的两个零调用更新函数与前端无发射端的刷新监听
        // 一并删除后不得回归。注释不得出现被断言标识符（§10.4 自命中陷阱）。
        let read_self = || {
            std::fs::read_to_string(format!(
                "{}/src/windows/pin_image_window/pin_image_window.rs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .expect("读取贴图窗口源码失败")
        };
        let code = read_self()
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 断言目标用拆分拼接构造：本测试读自己源码，若断言语句直接写完整
        // 标识符会自命中永远红（§10.4 自指陷阱）。
        let dead_fn_a = ["update_pin_", "image_file"].concat();
        let dead_fn_b = ["update_pin_", "image_data"].concat();
        let dead_event = ["pin-", "image:refresh"].concat();
        assert!(
            !code.contains(&dead_fn_a) && !code.contains(&dead_fn_b),
            "后端零调用更新函数不得重新引入"
        );
        assert!(!code.contains(&dead_event), "后端不得重新引入刷新事件发射");
        let front = std::fs::read_to_string(format!(
            "{}/../src/windows/pinImage/index.js",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图前端源码失败");
        assert!(!front.contains(&dead_event), "前端死监听不得重新引入");
    }

    #[test]
    fn lock_pin_data_recovers_from_poison() {
        let _g = lock_serial();

        // 先初始化 OnceCell
        init_pin_image_window();

        // 制造 poison:在 panic 线程里持锁
        let handle = std::thread::spawn(|| {
            let data_map = PIN_IMAGE_DATA_MAP
                .get_or_init(|| Mutex::new(HashMap::new()));
            let _guard = data_map.lock().unwrap();
            panic!("force PIN_IMAGE_DATA_MAP mutex poison");
        });
        let _ = handle.join();

        // helper 仍能拿到数据(不 panic)
        let map = lock_pin_data();
        assert!(map.is_empty(), "poison 后 PIN_IMAGE_DATA_MAP 仍可访问");
    }

    // 编辑模式阻塞桩:start_pin_edit_mode 必须直接返回不可用——
    // 旧实现注册一次性 "pin-edit-ready" 监听 + rx.recv_timeout(1s) 阻塞,
    // 超时才返回 Err;若前端在窗口生命周期内发射该事件,残留监听会执行
    // hide+缩到 1x1 的副作用。新实现不注册监听、不阻塞、不留挂起状态。
    #[test]
    fn pin_edit_mode_returns_unavailable_without_stub_side_effects() {
        let src = std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/pin_image_window.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图窗口源码失败");
        let body: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = body
            .find("pub async fn start_pin_edit_mode")
            .expect("缺 start_pin_edit_mode");
        let rest = &body[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(body.len());
        let fn_body = &body[start..end];
        // 桩的副作用全部不得再出现
        assert!(
            !fn_body.contains("recv_timeout"),
            "不得再阻塞等待 pin-edit-ready"
        );
        assert!(
            !fn_body.contains("app.once(\"pin-edit-ready\")"),
            "不得再注册一次性监听"
        );
        // 直接返回不可用
        assert!(
            fn_body.contains("Err(\"贴图编辑功能当前不可用\".to_string())"),
            "必须直接返回不可用"
        );
    }

    // 预览并发建窗数据错位:pin_image_from_file 的预览分支必须先取
    // PREVIEW_WINDOW_LOCK 再执行 close/insert/create 整个流程——固定标签
    // "image-preview" 下并发请求交错会导致窗口与其数据错位(详见函数注释)。
    // 护栏断言:取锁位于窗口标签确定之后、窗口创建调用之前。
    #[test]
    fn preview_window_creation_is_serialized_before_create() {
        let src = std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/pin_image_window.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图窗口源码失败");
        let stripped: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("pub async fn pin_image_from_file")
            .expect("缺 pin_image_from_file");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        // 锁必须被实际获取(变量名以 _preview_guard 开头,get_or_init 惰性初始化)
        let lock_pos = body
            .find("PREVIEW_WINDOW_LOCK")
            .expect("预览分支必须先取 PREVIEW_WINDOW_LOCK 串行化");
        // 取锁必须早于窗口创建(create_pin_image_window)——串行化覆盖整个建窗流程
        let create_pos = body
            .find("create_pin_image_window(&app, &window_label")
            .expect("必须创建贴图窗口");
        assert!(
            lock_pos < create_pos,
            "PREVIEW_WINDOW_LOCK 必须在 create_pin_image_window 之前获取"
        );
        // 取锁必须晚于标签确定——非预览窗口不走锁,锁只保护固定标签
        let label_pos = body
            .find("let window_label = if is_preview")
            .expect("必须先确定窗口标签");
        assert!(
            label_pos < lock_pos,
            "窗口标签确定必须先于取锁(锁只保护预览固定标签)"
        );
        // 取锁必须仅出现一次,且以 Some(PREVIEW_WINDOW_LOCK 形态被 if is_preview
        // 守卫——非预览分支 else 为 None,标签唯一无需串行化。
        assert_eq!(
            body.matches("PREVIEW_WINDOW_LOCK").count(),
            1,
            "取锁必须仅出现一次(仅预览分支)"
        );
        assert!(
            body.contains("Some(PREVIEW_WINDOW_LOCK"),
            "取锁必须位于 if is_preview 的 Some 分支(非预览为 None)"
        );
    }

    // close 未纳入串行化:close_image_preview 的 remove+close 必须
    // 与 pin_image_from_file 预览分支共用同一把 PREVIEW_WINDOW_LOCK——否则建窗
    // 流程 await create_pin_image_window 期间并发的 close(菜单 mouseleave/主窗
    // 隐藏路径)可移除刚 insert 的数据并关旧窗,建窗完成新窗渲染后其数据
    // 读 map 空返回 Err,预览窗空屏+穿透残留。close 是同步 fn,阻塞持有锁
    // 极短,不拖慢建窗。反证:删 close 的取锁块 → FAILED。
    #[test]
    fn close_image_preview_serialized_under_preview_lock() {
        let src = strip_line_comments(&source_file("src/windows/pin_image_window/pin_image_window.rs"));
        let close_body = fn_body(&src, "close_image_preview");
        let lock_pos = close_body
            .find("PREVIEW_WINDOW_LOCK")
            .expect("close_image_preview 必须取 PREVIEW_WINDOW_LOCK 串行化");
        let remove_pos = close_body
            .find("lock_pin_data().remove(label)")
            .expect("close 必须移除数据");
        let close_call_pos = close_body
            .find("PostMessageW")
            .expect("close 必须关闭窗口");
        assert!(
            lock_pos < remove_pos && remove_pos < close_call_pos,
            "close_image_preview 必须先取锁再 remove 再关闭窗口(与建窗流程互斥)"
        );
        // 负向:不能先 remove 再取锁(取锁必须在 remove 之前)
        assert!(
            !close_body[..lock_pos].contains("lock_pin_data().remove(label)"),
            "取锁必须早于数据移除"
        );
        // 负向(剥注释后):同步 fn 里不得出现裸 .lock()——tokio Mutex 的 lock()
        // 返回 future,同步 fn 从不 poll,锁形同未获取。必须 .blocking_lock()。
        assert!(
            !close_body.contains(").lock()"),
            "close_image_preview 同步 fn 必须用 blocking_lock,裸 lock() 从不被 poll"
        );
        assert!(
            close_body.contains(".blocking_lock()"),
            "close_image_preview 必须用 .blocking_lock() 同步获取预览锁"
        );
    }

    // 贴图窗口必须走 GDI 分层窗口(不再创建 WebView):
    // 创建路径必须调用 gdi::create_gdi_window,且不得再出现 WebviewWindowBuilder。
    #[test]
    fn pin_image_window_is_created_through_gdi() {
        let stripped: String = source_file("src/windows/pin_image_window/pin_image_window.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            stripped.contains("gdi::create_gdi_window"),
            "贴图窗口必须走 gdi::create_gdi_window(GDI 分层窗口)"
        );
        assert!(
            stripped.contains("super::gdi::render_image"),
            "建窗后必须渲染图片到 GDI 窗口"
        );
        assert!(
            !stripped.contains("WebviewWindowBuilder"),
            "贴图窗口不得再创建 WebView(WebviewWindowBuilder 已废弃)"
        );
    }

    // 预览窗口穿透由 GDI 层处理:pin_image_from_file 内不得再出现
    // set_ignore_cursor_events(WebView API, GDI 窗口无此方法)。
    #[test]
    fn preview_transparency_is_handled_in_gdi_layer() {
        let stripped: String = source_file("src/windows/pin_image_window/pin_image_window.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !stripped.contains("set_ignore_cursor_events"),
            "贴图穿透必须由 GDI 层(WS_EX_TRANSPARENT)处理,不得再调 WebView API"
        );
    }
}
