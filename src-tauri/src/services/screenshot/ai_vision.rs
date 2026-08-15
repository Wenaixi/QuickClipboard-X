use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    ColorType, ImageReader, ImageEncoder,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const MAX_IMAGE_DIMENSION: u32 = 4096;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const JPEG_QUALITY: u8 = 90;
const JPEG_MIN_QUALITY: u8 = 45;
const JPEG_QUALITY_STEP: u8 = 15;
const MAX_RECOGNITION_ATTEMPTS: usize = 2;
const DEFAULT_PROMPT: &str = "请完整转写图片中的文字，保留段落、换行和原有顺序；不要翻译、总结或补充图片中没有的信息；无法辨认的位置使用 [无法辨认] 标记。只返回 JSON，格式为 {\"text\":\"完整识别文本\",\"blocks\":[{\"type\":\"paragraph\",\"text\":\"段落文本\"}]}。";
const CONFIGURATION_TEST_PROMPT: &str = "请确认你可以读取这张测试图片。只返回 JSON，格式为 {\"text\":\"ok\",\"blocks\":[]}。";
const CONFIGURATION_TEST_IMAGE_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL9VwAAAABJRU5ErkJggg==";

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
    content: Vec<ChatContent<'a>>,
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

fn ensure_vision_model(model: &str) -> Result<(), AiVisionError> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(AiVisionError::NotConfigured);
    }
    let clearly_text_only = normalized.contains("instruct")
        && !normalized.contains("vision")
        && !normalized.contains("vl")
        && !normalized.contains("qwen2.5-vl");
    if clearly_text_only {
        return Err(AiVisionError::UnsupportedVisionModel);
    }
    Ok(())
}

pub fn validate_configuration(
    api_key: &str,
    base_url: &str,
    model: &str,
) -> Result<(), AiVisionError> {
    if api_key.trim().is_empty() {
        return Err(AiVisionError::NotConfigured);
    }
    normalized_base_url(base_url)?;
    ensure_vision_model(model)
}

pub async fn test_configuration(
    api_key: &str,
    base_url: &str,
    model: &str,
) -> Result<(), AiVisionError> {
    validate_configuration(api_key, base_url, model)?;
    let base_url = normalized_base_url(base_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AiVisionError::Request(String::new()))?;

    test_configuration_at_endpoint(
        &client,
        &format!("{base_url}/chat/completions"),
        api_key,
        model,
    )
    .await
}

async fn test_configuration_at_endpoint(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    model: &str,
) -> Result<(), AiVisionError> {
    recognize_encoded_image(
        client,
        endpoint,
        api_key,
        model,
        CONFIGURATION_TEST_PROMPT,
        CONFIGURATION_TEST_IMAGE_URL,
    )
    .await
    .map(|_| ())
}

fn classify_http_status(status: StatusCode) -> AiVisionError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => AiVisionError::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => AiVisionError::RateLimited,
        status if status.is_server_error() => AiVisionError::Server(status.as_u16()),
        status => AiVisionError::HttpStatus(status.as_u16()),
    }
}

fn encode_png(image: &image::DynamicImage) -> Result<Vec<u8>, AiVisionError> {
    let rgba = image.to_rgba8();
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(
            rgba.as_raw(),
            image.width(),
            image.height(),
            ColorType::Rgba8.into(),
        )
        .map_err(|error| AiVisionError::Encode(error.to_string()))?;
    Ok(bytes)
}

fn encode_jpeg(image: &image::DynamicImage, quality: u8) -> Result<Vec<u8>, AiVisionError> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(image)
        .map_err(|error| AiVisionError::Encode(error.to_string()))?;
    Ok(bytes)
}

fn encode_dynamic_image(image: image::DynamicImage) -> Result<(String, String), AiVisionError> {
    let image = if image.width() > MAX_IMAGE_DIMENSION || image.height() > MAX_IMAGE_DIMENSION {
        image.thumbnail(MAX_IMAGE_DIMENSION, MAX_IMAGE_DIMENSION)
    } else {
        image
    };

    let png_bytes = encode_png(&image)?;
    if png_bytes.len() <= MAX_IMAGE_BYTES {
        return Ok(("image/png".to_string(), BASE64.encode(png_bytes)));
    }

    let mut quality = JPEG_QUALITY;
    while quality >= JPEG_MIN_QUALITY {
        let jpeg_bytes = encode_jpeg(&image, quality)?;
        if jpeg_bytes.len() <= MAX_IMAGE_BYTES {
            return Ok(("image/jpeg".to_string(), BASE64.encode(jpeg_bytes)));
        }
        quality = quality.saturating_sub(JPEG_QUALITY_STEP);
    }

    Err(AiVisionError::InvalidImage("图片超过 8 MB 限制".to_string()))
}

fn encode_image(path: &Path) -> Result<(String, String), AiVisionError> {
    let image = ImageReader::open(path)
        .map_err(|error| AiVisionError::InvalidImage(error.to_string()))?
        .decode()
        .map_err(|error| AiVisionError::InvalidImage(error.to_string()))?;
    encode_dynamic_image(image)
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

async fn recognize_encoded_image(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    image_url: &str,
) -> Result<AiVisionResult, AiVisionError> {
    for attempt in 0..MAX_RECOGNITION_ATTEMPTS {
        let retry_hint = if attempt == 0 {
            ""
        } else {
            "上一条回复不是合法 JSON。请重新识别，并且只返回合法 JSON 对象，不要添加 Markdown、解释或代码围栏。"
        };
        let mut contents = vec![
            ChatContent::Text { text: prompt },
            ChatContent::ImageUrl { image_url: ImageUrl { url: &image_url } },
        ];
        if !retry_hint.is_empty() {
            contents.push(ChatContent::Text { text: retry_hint });
        }
        let request = ChatCompletionRequest {
            model,
            messages: [ChatMessageRequest {
                role: "user",
                content: contents,
            }],
            temperature: 0.0,
        };

        let response = client
            .post(endpoint)
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
        match parse_result(&content) {
            Ok(result) => return Ok(result),
            Err(_error) if attempt + 1 < MAX_RECOGNITION_ATTEMPTS => continue,
            Err(error) => return Err(error),
        }
    }

    Err(AiVisionError::InvalidResponse("响应无法解析".to_string()))
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
    ensure_vision_model(model)?;

    let base_url = normalized_base_url(base_url)?;
    let image_path = path.to_path_buf();
    let (mime, encoded) = tokio::task::spawn_blocking(move || encode_image(&image_path))
        .await
        .map_err(|_| AiVisionError::Request(String::new()))??;
    let image_url = format!("data:{mime};base64,{encoded}");
    let prompt = prompt.filter(|value| !value.trim().is_empty()).unwrap_or(DEFAULT_PROMPT);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AiVisionError::Request(String::new()))?;

    recognize_encoded_image(
        &client,
        &format!("{base_url}/chat/completions"),
        api_key,
        model,
        prompt,
        &image_url,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        matchers::{bearer_token, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn base_url_is_normalized_to_openai_compatible_v1_endpoint() {
        assert_eq!(normalized_base_url("https://example.com"), Ok("https://example.com/v1".to_string()));
        assert_eq!(normalized_base_url("https://example.com/v1/"), Ok("https://example.com/v1".to_string()));
        assert!(matches!(normalized_base_url("http://example.com"), Err(AiVisionError::NotConfigured)));
    }

    #[test]
    fn clearly_non_vision_models_are_rejected_before_network_request() {
        assert_eq!(ensure_vision_model("Qwen/Qwen2-7B-Instruct"), Err(AiVisionError::UnsupportedVisionModel));
        assert!(ensure_vision_model("Qwen/Qwen2.5-VL-7B-Instruct").is_ok());
        assert!(ensure_vision_model("gpt-4o").is_ok());
    }

    #[test]
    fn configuration_validation_requires_key_https_endpoint_and_vision_model() {
        assert!(validate_configuration(
            "test-key",
            "https://api.example.com/v1",
            "Qwen/Qwen2.5-VL-7B-Instruct",
        )
        .is_ok());
        assert_eq!(
            validate_configuration("", "https://api.example.com", "gpt-4o"),
            Err(AiVisionError::NotConfigured)
        );
        assert_eq!(
            validate_configuration("test-key", "http://api.example.com", "gpt-4o"),
            Err(AiVisionError::NotConfigured)
        );
        assert_eq!(
            validate_configuration("test-key", "https://api.example.com", "Qwen/Qwen2-7B-Instruct"),
            Err(AiVisionError::UnsupportedVisionModel)
        );
    }

    #[tokio::test]
    async fn configuration_test_uses_a_fixed_tiny_image_and_does_not_read_user_files() {
        let server = MockServer::start().await;
        let guard = Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "{\"text\":\"ok\",\"blocks\":[]}" } }]
            })))
            .mount_as_scoped(&server)
            .await;

        test_configuration_at_endpoint(
            &reqwest::Client::new(),
            &format!("{}/v1/chat/completions", server.uri()),
            "test-key",
            "vision-model",
        )
        .await
        .unwrap();

        let requests = guard.received_requests().await;
        let body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(body["messages"][0]["content"][1]["image_url"]["url"], CONFIGURATION_TEST_IMAGE_URL);
    }

    #[test]
    fn response_parser_accepts_one_constrained_json_extraction() {
        let result = parse_result("以下是结果：{\"text\":\"你好\",\"blocks\":[]}").unwrap();
        assert_eq!(result.text, "你好");
        assert!(matches!(parse_result("无法识别"), Err(AiVisionError::InvalidResponse(_))));
    }

    #[test]
    fn small_images_use_png_data_before_jpeg_fallback() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([1, 2, 3, 255]),
        ));
        let (mime, encoded) = encode_dynamic_image(image).unwrap();
        let bytes = BASE64.decode(encoded).unwrap();

        assert_eq!(mime, "image/png");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn oversized_png_falls_back_to_jpeg_before_rejecting_image() {
        let width = 2_048;
        let height = 2_048;
        let mut state = 0x1234_5678_u32;
        let pixels = (0..(width * height * 4))
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let image = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(width, height, pixels).unwrap(),
        );
        let (mime, encoded) = encode_dynamic_image(image).unwrap();
        let bytes = BASE64.decode(encoded).unwrap();

        assert_eq!(mime, "image/jpeg");
        assert_eq!(&bytes[..2], b"\xff\xd8");
    }

    #[test]
    fn http_statuses_are_classified_without_response_body() {
        assert_eq!(classify_http_status(StatusCode::UNAUTHORIZED), AiVisionError::Unauthorized);
        assert_eq!(classify_http_status(StatusCode::TOO_MANY_REQUESTS), AiVisionError::RateLimited);
        assert_eq!(classify_http_status(StatusCode::BAD_GATEWAY), AiVisionError::Server(502));
        assert_eq!(classify_http_status(StatusCode::BAD_REQUEST), AiVisionError::HttpStatus(400));
    }

    #[tokio::test]
    async fn ai_request_sends_bearer_model_prompt_and_png_data_url_to_local_mock() {
        let server = MockServer::start().await;
        let guard = Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(bearer_token("test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "content": "{\"text\":\"识别结果\",\"blocks\":[]}"
                    }
                }]
            })))
            .mount_as_scoped(&server)
            .await;
        let client = reqwest::Client::new();

        let result = recognize_encoded_image(
            &client,
            &format!("{}/v1/chat/completions", server.uri()),
            "test-key",
            "vision-model",
            "请转写",
            "data:image/png;base64,AAAA",
        )
        .await
        .unwrap();

        assert_eq!(result.text, "识别结果");
        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(body["model"], "vision-model");
        assert_eq!(body["messages"][0]["content"][0]["text"], "请转写");
        assert_eq!(body["messages"][0]["content"][1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[tokio::test]
    async fn ai_request_retries_once_with_format_correction_after_invalid_json() {
        let server = MockServer::start().await;
        let first = Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "不是 JSON" } }]
            })))
            .up_to_n_times(1)
            .mount_as_scoped(&server)
            .await;
        let second = Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "{\"text\":\"重试成功\",\"blocks\":[]}" }
                }]
            })))
            .mount_as_scoped(&server)
            .await;

        let result = recognize_encoded_image(
            &reqwest::Client::new(),
            &format!("{}/v1/chat/completions", server.uri()),
            "test-key",
            "vision-model",
            "请转写",
            "data:image/png;base64,AAAA",
        )
        .await
        .unwrap();

        assert_eq!(result.text, "重试成功");
        assert_eq!(first.received_requests().await.len(), 1);
        let retried_requests = second.received_requests().await;
        assert_eq!(retried_requests.len(), 1);
        let body: serde_json::Value = retried_requests[0].body_json().unwrap();
        assert_eq!(body["messages"][0]["content"][2]["type"], "text");
        assert!(body["messages"][0]["content"][2]["text"]
            .as_str()
            .unwrap()
            .contains("不是合法 JSON"));
    }

    #[tokio::test]
    async fn ai_request_maps_local_auth_rate_limit_and_server_failures() {
        for (status, expected) in [
            (401, AiVisionError::Unauthorized),
            (429, AiVisionError::RateLimited),
            (502, AiVisionError::Server(502)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let error = recognize_encoded_image(
                &reqwest::Client::new(),
                &format!("{}/v1/chat/completions", server.uri()),
                "test-key",
                "vision-model",
                "请转写",
                "data:image/png;base64,AAAA",
            )
            .await
            .unwrap_err();
            assert_eq!(error, expected);
        }
    }

    #[tokio::test]
    async fn ai_request_maps_local_delayed_response_to_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(1))
            .build()
            .unwrap();

        let error = recognize_encoded_image(
            &client,
            &format!("{}/v1/chat/completions", server.uri()),
            "test-key",
            "vision-model",
            "请转写",
            "data:image/png;base64,AAAA",
        )
        .await
        .unwrap_err();

        assert_eq!(error, AiVisionError::Timeout);
    }

    #[test]
    fn errors_do_not_include_credentials_or_raw_response() {
        let error = AiVisionError::Request("secret-key raw response".to_string()).to_string();
        assert!(!error.contains("secret-key"));
        assert!(!error.contains("raw response"));
        assert_eq!(AiVisionError::Timeout.to_string(), "AI 视觉识别请求超时");
    }
}
