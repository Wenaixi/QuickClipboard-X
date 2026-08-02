/// 恢复保存的窗口逻辑尺寸:宽高均至少 150 逻辑像素。
/// 两处调用点(贴边启动恢复 / 普通 show 恢复)共用,首参曾传 scale_factor/window
/// 但函数体从未使用,已删(YAGNI)。
pub fn normalize_saved_window_size(width: u32, height: u32) -> (f64, f64) {
    (width.max(150) as f64, height.max(150) as f64)
}

#[cfg(test)]
mod tests {
    use super::normalize_saved_window_size;

    #[test]
    fn clamps_below_minimum_to_150() {
        assert_eq!(normalize_saved_window_size(100, 80), (150.0, 150.0));
    }

    #[test]
    fn keeps_sizes_at_or_above_minimum() {
        assert_eq!(normalize_saved_window_size(360, 520), (360.0, 520.0));
        assert_eq!(normalize_saved_window_size(150, 150), (150.0, 150.0));
    }
}
