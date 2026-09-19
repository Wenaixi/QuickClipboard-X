// R6 取色器颜色服务（对齐 ShareX ScreenColorPickerWindow 的 GetPixel 取色）：
// 取屏幕指定位置像素颜色并转多种色值格式。命令薄封装 + 纯函数易测。
// 取色失败返回 None（句柄无效/坐标越界等），颜色用 0xRRGGBB 表示。
// 色值转换（Hex/Decimal/HSB/CMYK）归前端 colorModel.js 负责，后端只出 RGB。

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};

/// 屏幕指定位置的像素颜色（0xRRGGBB）。取屏失败返回 None。
pub fn screen_color_at(x: i32, y: i32) -> Option<u32> {
    let hdc = unsafe { GetDC(None) };
    if hdc.is_invalid() {
        return None;
    }
    let packed = unsafe { GetPixel(hdc, x, y) };
    unsafe {
        let _ = ReleaseDC(None, hdc);
    }
    // GetPixel 返回 COLORREF（0x00BBGGRR）：失 败/越界时返回 CLR_INVALID (0xFFFFFFFF)。
    if packed == u32::MAX {
        return None;
    }
    let blue = (packed >> 16) & 0xFF;
    let green = (packed >> 8) & 0xFF;
    let red = packed & 0xFF;
    Some((red << 16) | (green << 8) | blue)
}

/// RGB → 小写十六进制字符串（"rrggbb"）。
pub fn rgb_to_hex(rgb: u32) -> String {
    format!("{:06x}", rgb & 0xFFFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_to_hex_round_trips_known_colors() {
        assert_eq!(rgb_to_hex(0xFF0000), "ff0000");
        assert_eq!(rgb_to_hex(0x00FF00), "00ff00");
        assert_eq!(rgb_to_hex(0x0000FF), "0000ff");
        assert_eq!(rgb_to_hex(0xFFFFFF), "ffffff");
        assert_eq!(rgb_to_hex(0), "000000");
    }

    #[test]
    fn screen_color_reads_through_get_pixel_lock() {
        // 源码护栏：取色必须走 GDI GetPixel + GetDC/ReleaseDC 配对；
        // 失败（CLR_INVALID）必须返回 None 而非 0（0 是合法黑色）。
        let source = std::fs::read_to_string(format!(
            "{}/src/services/tools/color.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取取色服务源码失败");
        assert!(source.contains("GetPixel(hdc, x, y)"), "必须走 GetPixel 取色");
        assert!(source.contains("GetDC(None)"), "必须获取屏幕 DC");
        assert!(source.contains("ReleaseDC(None, hdc)"), "DC 必须配对释放");
        assert!(source.contains("packed == u32::MAX"), "CLR_INVALID 必须判失败");
        assert!(source.contains("pub fn screen_color_at(x: i32, y: i32)"), "取色函数必须存在定义");
    }
}