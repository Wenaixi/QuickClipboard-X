use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::OnceCell;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Listener, Manager, WebviewWindow, WebviewWindowBuilder, Size, LogicalSize, PhysicalPosition, PhysicalSize};

static PIN_IMAGE_COUNTER: AtomicUsize = AtomicUsize::new(0);
static PIN_IMAGE_DATA_MAP: OnceCell<Mutex<HashMap<String, PinImageData>>> = OnceCell::new();

// 锁 helper:OnceCell + Mutex poison 双重处理
// ponytail:9 个调用点都走这里,统一 poison 恢复语义
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
        // C1:x/y 分支窗口落在指定坐标所在显示器——图片逻辑尺寸必须按该屏
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

    if is_preview {
        if let Some(existing) = app.get_webview_window(&window_label) {
            let _ = existing.close();
            lock_pin_data().remove(&window_label);
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
    
    let window = create_pin_image_window(&app, &window_label, img_width, img_height, pos_x, pos_y).await?;
    
    if is_preview {
        window.set_ignore_cursor_events(true).map_err(|e| format!("设置鼠标穿透失败: {}", e))?;
    }
    
    window.show().map_err(|e| format!("显示贴图窗口失败: {}", e))?;
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

// C1:按目标坐标所在显示器折算图片逻辑尺寸——贴图窗口用 inner_size(logical)
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
) -> Result<WebviewWindow, String> {
    let window = WebviewWindowBuilder::new(
        app, label,
        tauri::WebviewUrl::App("windows/pinImage/pinImage.html".into()),
    )
    .title("贴图")
    .inner_size(width as f64 + 10.0, height as f64 + 10.0)
    .min_inner_size(1.0, 1.0)
    .resizable(false)
    .maximizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .drag_and_drop(false)
    .build()
    .map_err(|e| format!("创建贴图窗口失败: {}", e))?;
    
    window.set_position(PhysicalPosition::new(physical_x, physical_y))
        .map_err(|e| format!("设置窗口位置失败: {}", e))?;
    
    Ok(window)
}

// 前端请求获取图片数据
#[tauri::command]
pub fn get_pin_image_data(window: WebviewWindow) -> Result<serde_json::Value, String> {
    let map = lock_pin_data();
    if let Some(data) = map.get(window.label()) {
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


// 图片另存为
#[tauri::command]
pub async fn save_pin_image_as(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    use std::path::Path;
    use tauri_plugin_dialog::DialogExt;
    
    let file_path = {
        let map = lock_pin_data();
        if let Some(data) = map.get(window.label()) {
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
#[tauri::command]
pub fn close_image_preview(app: AppHandle) -> Result<(), String> {
    let label = "image-preview";
    if let Some(window) = app.get_webview_window(label) {
        lock_pin_data().remove(label);
        let _ = window.hide();
        let _ = window.set_size(Size::Logical(LogicalSize::new(1.0, 1.0)));
        let _ = window.close();
    }
    Ok(())
}

// 关闭贴图窗口
#[tauri::command]
pub fn close_pin_image_window_by_self(window: WebviewWindow) -> Result<(), String> {
    let label = window.label().to_string();

    let mut map = lock_pin_data();
    if let Some(data) = map.remove(&label) {
        if let Some(ref original_path) = data.original_image_path {
            if data.file_path != *original_path {
                let file_in_use = map.values().any(|d| d.file_path == data.file_path);
                if !file_in_use {
                    let _ = std::fs::remove_file(&data.file_path);
                }
            }
        }
    }
    drop(map);

    let _ = window.set_size(Size::Logical(LogicalSize::new(1.0, 1.0)));
    window.close().map_err(|e| format!("关闭窗口失败: {}", e))?;

    Ok(())
}

// 启动贴图编辑模式
#[tauri::command]
pub async fn start_pin_edit_mode(
    _app: AppHandle,
    _window: WebviewWindow,
    _img_offset_x_physical: i32,
    _img_offset_y_physical: i32,
    _img_width_physical: u32,
    _img_height_physical: u32,
) -> Result<(), String> {
    // w5:贴图编辑功能当前未实现——旧实现是 1s 阻塞桩:注册一次性的
    // "pin-edit-ready" 监听 + rx.recv_timeout(1s) 阻塞等待,超时后返回
    // Err,还在 app.once 里残留监听(窗口被前端发射事件时会执行 hide+缩到
    // 1x1 的副作用)。直接返回不可用,不注册监听、不阻塞、不留挂起状态。
    Err("贴图编辑功能当前不可用".to_string())
}

#[tauri::command]
pub fn animate_window_resize(
    window: WebviewWindow,
    start_w: f64, start_h: f64,
    start_x: i32, start_y: i32,
    end_w: f64, end_h: f64,
    end_x: i32, end_y: i32,
    duration_ms: u64,
) -> Result<(), String> {
    let window = window.clone();
    
    tauri::async_runtime::spawn(async move {
        let start_time = Instant::now();
        let duration = Duration::from_millis(duration_ms);
        let frame_duration = Duration::from_millis(16);

        let dw = end_w - start_w;
        let dh = end_h - start_h;
        let dx = end_x - start_x;
        let dy = end_y - start_y;

        loop {
            let elapsed = start_time.elapsed();
            if elapsed >= duration {
                let _ = window.set_size(PhysicalSize::new(end_w as u32, end_h as u32));
                let _ = window.set_position(PhysicalPosition::new(end_x, end_y));
                break;
            }

            let progress = elapsed.as_secs_f64() / duration.as_secs_f64();

            let eased = 1.0 - 2f64.powf(-10.0 * progress);

            let cur_w = (start_w + dw * eased).round() as u32;
            let cur_h = (start_h + dh * eased).round() as u32;
            let cur_x = start_x + (dx as f64 * eased).round() as i32;
            let cur_y = start_y + (dy as f64 * eased).round() as i32;

            let _ = window.set_size(PhysicalSize::new(cur_w, cur_h));
            let _ = window.set_position(PhysicalPosition::new(cur_x, cur_y));

            tokio::time::sleep(frame_duration).await;
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // §11.2:全局静态测试串行化(同 §7.12 state.rs SERIAL 模式)
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    // C1(贴图 scale 用错屏):x/y 与 center 分支的图片逻辑尺寸必须按目标
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

    // w5(编辑模式阻塞桩):start_pin_edit_mode 必须直接返回不可用——
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
}
