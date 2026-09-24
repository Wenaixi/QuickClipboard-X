//! 文件剪贴板数据结构（FileInfo/FilesData），纯 serde 无平台依赖

use serde::{Deserialize, Serialize};

// 文件信息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub is_directory: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data: Option<String>,
    #[serde(default)]
    pub file_type: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

// 文件剪贴板数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesData {
    pub files: Vec<FileInfo>,
    #[serde(default)]
    pub operation: String,
}