// R4 屏幕录制 GIF 编码器（零新依赖：image crate 已开 gif feature）。
// 纯函数无 I/O：把 RGBA 帧序列编码为 GIF 字节。对齐 ShareX ScreenRecorder
// 的 GIF 输出语义——每帧全量编码（不做差分），量化由调用方按参数决定
//（帧率/质量在调用侧调节，本模块只管帧序列 → GIF）。

use image::{codecs::gif::GifEncoder, Delay, Frame, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifFrameRate {
    pub frames_per_second: u8,
}

impl GifFrameRate {
    /// 帧率必须落在 R4 允许范围 10~15fps（对齐 ShareX 录制默认帧率）。
    pub fn new(frames_per_second: u8) -> Result<Self, String> {
        if !(10..=15).contains(&frames_per_second) {
            return Err(format!("GIF 帧率必须在 10~15fps 之间，收到 {frames_per_second}"));
        }
        Ok(Self { frames_per_second })
    }

    pub fn delay_ms(self) -> u32 {
        (1000 / self.frames_per_second as u32).max(1)
    }
}

/// 把 RGBA 帧序列编码为 GIF 字节（每帧尺寸必须一致，含至少 1 帧）。
pub fn encode_rgba_frames(frames: &[(u32, u32, &[u8])], frame_rate: GifFrameRate) -> Result<Vec<u8>, String> {
    if frames.is_empty() {
        return Err("GIF 至少需要 1 帧".to_string());
    }
    let (width, height, _) = frames[0];
    if width == 0 || height == 0 {
        return Err("GIF 帧尺寸无效".to_string());
    }
    let delay = Delay::from_millis(frame_rate.delay_ms());
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        for (frame_width, frame_height, rgba) in frames {
            if *frame_width != width || *frame_height != height {
                return Err("GIF 各帧尺寸必须一致".to_string());
            }
            let expected = (width as usize) * (height as usize) * 4;
            if rgba.len() != expected {
                return Err("GIF 帧像素数据长度无效".to_string());
            }
            let image = RgbaImage::from_raw(width, height, rgba.to_vec())
                .ok_or_else(|| "GIF 帧像素缓冲无效".to_string())?;
            encoder
                .encode_delay(Frame::new(image), delay)
                .map_err(|error| format!("GIF 编码失败: {error}"))?;
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_frame(width: u32, height: u32, rgba: u8) -> Vec<u8> {
        vec![rgba; (width * height * 4) as usize]
    }

    #[test]
    fn frame_rate_restricted_to_recording_range() {
        assert!(GifFrameRate::new(10).is_ok());
        assert!(GifFrameRate::new(15).is_ok());
        assert!(GifFrameRate::new(9).is_err(), "低于 10fps 必须拒绝");
        assert!(GifFrameRate::new(16).is_err(), "高于 15fps 必须拒绝");
        assert_eq!(GifFrameRate::new(10).unwrap().delay_ms(), 100);
        assert_eq!(GifFrameRate::new(15).unwrap().delay_ms(), 66);
    }

    #[test]
    fn empty_frame_list_rejected() {
        assert!(encode_rgba_frames(&[], GifFrameRate::new(10).unwrap()).is_err());
    }

    #[test]
    fn mismatched_frame_sizes_rejected() {
        let frames = [(10, 10, solid_frame(10, 10, 255).as_slice()), (11, 10, solid_frame(11, 10, 255).as_slice())];
        assert!(encode_rgba_frames(&frames, GifFrameRate::new(10).unwrap()).is_err());
    }

    #[test]
    fn invalid_pixel_length_rejected() {
        let frames = [(2, 2, &[1u8, 2, 3][..])];
        assert!(encode_rgba_frames(&frames, GifFrameRate::new(10).unwrap()).is_err());
    }

    #[test]
    fn single_frame_encodes_with_gif_signature() {
        let frame = solid_frame(2, 2, 200);
        let gif = encode_rgba_frames(&[(2, 2, frame.as_slice())], GifFrameRate::new(10).unwrap()).expect("编码失败");
        assert!(gif.len() > 10);
        assert_eq!(&gif[..6], b"GIF89a", "GIF 必须以 GIF89a 签名开头");
    }
}