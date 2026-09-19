// R6 工具集模块入口：color（取色器吸管）与 qr（二维码生成）先行，
// 后续哈希/标尺/白板在此扩展。

pub mod color;
pub mod qr;

pub use qr::store_qr_png_base64;
