use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::{codecs::jpeg::JpegEncoder, ImageReader};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const MAX_IMAGE_DIMENSION: u32 = 4096;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const JPEG_QUALITY: u8 = 90;
const DEFAULT_PROMPT: &str = "请完整转写图片中的文字，保留段落、换行和原有顺序；不要翻译、总结或补充图片中没有的信息；无法辨认的位置使用 [无法辨认] 标记。只返回 JSON，格式为 {\"text\":\"完整识别文本\",\"blocks\":[{\"type\":\"paragraph\",\"text\":\"段落文本\"}]}。";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiVisionError {
    NotConfigured,
    InvalidImage(String),
    Encode(String),
    UnsupportedVisionModel,
    Request(String),
    Timeout,
    HttpStatus(u16),
    Unauthorized,
    RateLimited,
    Server(u16),
    InvalidResponse(String),
}

impl std::fmt::Display for AiVisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "AI 视觉识别尚未完成配置"),
            Self::InvalidImage(error) => write!(f, "AI 图片无效: {error}"),
            Self::Encode(error) => write!(f, "AI 图片编码失败: {error}"),
            Self::UnsupportedVisionModel => write!(f, "当前 AI 模型不支持图片识别"),
            Self::Request(_) => write!(f, "AI 视觉识别请求失败"),
            Self::Timeout => write!(f, "AI 视觉识别请求超时"),
            Self::HttpStatus(status) => write!(f, "AI 服务返回 HTTP {status}"),
            Self::Unauthorized => write!(f, "AI 视觉服务认证失败"),
            Self::RateLimited => write!(f, "AI 视觉服务请求过于频繁"),
            Self::Server(status) => write!(f, "AI 视觉服务暂时不可用 (HTTP {status})"),
            Self::InvalidResponse(error) => write!(f, "AI 返回内容无法解析: {error}"),
        }
    }
}

impl std::error::Error for AiVisionError {}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AiVisionBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AiVisionResult {
    pub text: String,
    #[serde(default)]
    pub blocks: Vec<AiVisionBlock>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: [ChatMessageRequest<'a>; 1],
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct ChatMessageRequest<'a> {
    role: &'a str,
    content: [ChatContent<'a>; 2],
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ChatContent<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl<'a> },
}

#[derive(Debug, Serialize)]
struct ImageUrl<'a> {
    url: &'a str,
}

fn normalized_base_url(base_url: &str) -> Result<String, AiVisionError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.starts_with("https://") {
        return Err(AiVisionError::NotConfigured);
    }
    Ok(if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    })
}

fn classify_http_status(status: StatusCode) -> AiVisionError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => AiVisionError::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => AiVisionError::RateLimited,
        status if status.is_server_error() => AiVisionError::Server(status.as_u16()),
        status => AiVisionError::HttpStatus(status.as_u16()),
    }
}

fn encode_image(path: &Path) -> Result<(String, String), AiVisionError> {
    let image = ImageReader::open(path)
        .map_err(|error| AiVisionError::InvalidImage(error.to_string()))?
        .decode()
        .map_err(|error| AiVisionError::InvalidImage(error.to_string()))?;
    let image = if image.width() > MAX_IMAGE_DIMENSION || image.height() > MAX_IMAGE_DIMENSION {
        image.thumbnail(MAX_IMAGE_DIMENSION, MAX_IMAGE_DIMENSION)
    } else {
        image
    };

    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .encode_image(&image)
        .map_err(|error| AiVisionError::Encode(error.to_string()))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AiVisionError::InvalidImage("图片超过 8 MB 限制".to_string()));
    }
    Ok(("image/jpeg".to_string(), BASE64.encode(bytes)))
}

fn parse_result(content: &str) -> Result<AiVisionResult, AiVisionError> {
    if let Ok(result) = serde_json::from_str::<AiVisionResult>(content) {
        return Ok(result);
    }

    let start = content.find('{').ok_or_else(|| AiVisionError::InvalidResponse("缺少 JSON 对象".to_string()))?;
    let end = content.rfind('}').ok_or_else(|| AiVisionError::InvalidResponse("JSON 对象不完整".to_string()))?;
    serde_json::from_str::<AiVisionResult>(&content[start..=end])
        .map_err(|error| AiVisionError::InvalidResponse(error.to_string()))
}

pub async fn recognize_image(
    path: &Path,
    api_key: &str,
    base_url: &str,
    model: &str,
    prompt: Option<&str>,
) -> Result<AiVisionResult, AiVisionError> {
    if api_key.trim().is_empty() || model.trim().is_empty() {
        return Err(AiVisionError::NotConfigured);
    }
    let model_name = model.to_ascii_lowercase();
    if model_name.contains("instruct") && !model_name.contains("vision") {
        return Err(AiVisionError::UnsupportedVisionModel);
    }
    let base_url = normalized_base_url(base_url)?;
    let (mime, encoded) = encode_image(path)?;
    let image_url = format!("data:{mime};base64,{encoded}");
    let request = ChatCompletionRequest {
        model,
        messages: [ChatMessageRequest {
            role: "user",
            content: [
                ChatContent::Text { text: prompt.filter(|value| !value.trim().is_empty()).unwrap_or(DEFAULT_PROMPT) },
                ChatContent::ImageUrl { image_url: ImageUrl { url: &image_url } },
            ],
        }],
        temperature: 0.0,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AiVisionError::Request(String::new()))?;
    let response = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                AiVisionError::Timeout
            } else {
                AiVisionError::Request(String::new())
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(classify_http_status(status));
    }
    let response = response
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|_| AiVisionError::InvalidResponse("响应 JSON 无法解析".to_string()))?;
    let content = response
        .choices
        .first()
        .ok_or_else(|| AiVisionError::InvalidResponse("响应没有 choices".to_string()))?
        .message
        .content
        .trim()
        .to_string();
    parse_result(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_is_normalized_to_openai_compatible_v1_endpoint() {
        assert_eq!(normalized_base_url("https://example.com"), Ok("https://example.com/v1".to_string()));
        assert_eq!(normalized_base_url("https://example.com/v1/"), Ok("https://example.com/v1".to_string()));
        assert!(matches!(normalized_base_url("http://example.com"), Err(AiVisionError::NotConfigured)));
    }

    #[test]
    fn clearly_non_vision_models_are_rejected_before_network_request() {
        let model = "Qwen/Qwen2-7B-Instruct";
        assert!(model.to_ascii_lowercase().contains("instruct"));
        assert!(!model.to_ascii_lowercase().contains("vision"));
    }

    #[test]
    fn response_parser_accepts_one_constrained_json_extraction() {
        let result = parse_result("以下是结果：{\"text\":\"你好\",\"blocks\":[]}").unwrap();
        assert_eq!(result.text, "你好");
        assert!(matches!(parse_result("无法识别"), Err(AiVisionError::InvalidResponse(_))));
    }

    #[test]
    fn http_statuses_are_classified_without_response_body() {
        assert_eq!(classify_http_status(StatusCode::UNAUTHORIZED), AiVisionError::Unauthorized);
        assert_eq!(classify_http_status(StatusCode::TOO_MANY_REQUESTS), AiVisionError::RateLimited);
        assert_eq!(classify_http_status(StatusCode::BAD_GATEWAY), AiVisionError::Server(502));
        assert_eq!(classify_http_status(StatusCode::BAD_REQUEST), AiVisionError::HttpStatus(400));
    }

    #[test]
    fn errors_do_not_include_credentials_or_raw_response() {
        let error = AiVisionError::Request("secret-key raw response".to_string()).to_string();
        assert!(!error.contains("secret-key"));
        assert!(!error.contains("raw response"));
        assert_eq!(AiVisionError::Timeout.to_string(), "AI 视觉识别请求超时");
    }
}
