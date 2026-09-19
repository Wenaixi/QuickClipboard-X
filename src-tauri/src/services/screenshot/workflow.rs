// 截图动作链引擎
//
// 深度参考 ShareX WorkerTask.cs(已读源码):DoAfterCaptureJobs 按位标志
// 顺序执行 Beautify→Effects→Annotate→Copy→Pin→Save→Upload,每个动作
// 各自 try/catch 不中断,错误累加进 Info.Result.Errors,任务最终按
// IsError 置 Failed 但已生效动作结果保留,ShowErrorWindow 单独展示错误
// 列表。本引擎用有序 step 列表表达同样的"顺序+失败继续+汇总"语义:
// 列表可表达顺序/依赖,位标志只能表达集合;失败策略=每步独立收集,
// 截图不因某步失败而丢,结束时按 is_total_failure 决定是否整体失败。
//
// 动作实现内聚本文件(对齐 ShareX 自带 DoAfterCaptureJobs 实现),会话
// 守卫/提交由调用方以回调注入(screenshot_window 的 is_current_processing_
// session / begin_screenshot_commit),引擎只做流水线与动作分发。

use tauri::AppHandle;

use super::actions::{copy_screenshot, copy_screenshot_text, emit_screenshot_history_update, save_screenshot};
use super::ai_vision::recognize_image;
use super::image_store::prepare_pin_path;
use super::StoredScreenshot;
use crate::services::settings::get_settings;

/// 单步动作及其参数
#[derive(Debug, Clone)]
pub struct WorkflowStep {
    pub action: String,
}

impl WorkflowStep {
    pub fn new(action: impl Into<String>) -> Self {
        Self { action: action.into() }
    }
}

/// 动作链汇总结果(对齐 ShareX Info.Result.Errors 累加语义)
#[derive(Debug, Clone)]
pub struct WorkflowActionResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl WorkflowActionResult {
    /// 是否完全失败(无任何成功步骤)——对齐 ShareX Status=Failed
    pub fn is_total_failure(&self) -> bool {
        self.succeeded.is_empty()
    }

    /// 汇总展示文本(对齐 ShareX ShowErrorWindow 的 ErrorsToString):
    /// 完全失败返回错误明细,部分成功返回"部分动作成功"提示。
    pub fn error_summary(&self) -> String {
        if self.failed.is_empty() {
            return String::new();
        }
        let details: Vec<String> = self
            .failed
            .iter()
            .map(|(action, error)| format!("{}: {}", action, error))
            .collect();
        if self.is_total_failure() {
            format!("截图动作全部失败:\n{}", details.join("\n"))
        } else {
            format!("部分动作失败(其余已生效):\n{}", details.join("\n"))
        }
    }
}

/// 执行截图后动作链:遍历步骤,每步失败继续,汇总返回。
/// 会话守卫/提交回调由调用方注入;动作实现内聚本文件。
pub async fn execute_workflow(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    steps: &[WorkflowStep],
    is_processing: impl Fn(&str) -> bool + Sync,
    begin_commit: impl Fn(&str) -> Result<(), String> + Sync,
) -> WorkflowActionResult {
    let mut result = WorkflowActionResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for step in steps {
        let step_result: Result<String, String> = match step.action.as_str() {
            "copy" => run_copy_action(app, session_id, stored, &is_processing, &begin_commit).await,
            "save" => run_save_action(app, session_id, stored, &is_processing, &begin_commit).await,
            "pin" => run_pin_action(app, session_id, stored, &is_processing, &begin_commit).await,
            "ai" => run_ai_action(app, session_id, stored, &is_processing, &begin_commit).await,
            "edit" => run_edit_action(app, session_id, stored, &is_processing).await,
            "upload" => run_upload_action(app, session_id, stored, &is_processing, &begin_commit).await,
            "copy+pin" => run_copy_pin_action(app, session_id, stored, &is_processing, &begin_commit).await,
            other => Err(format!("不支持的截图动作: {other}")),
        };

        match step_result {
            Ok(summary) => result.succeeded.push(summary),
            Err(error) => result.failed.push((step.action.clone(), error)),
        }
    }

    result
}

// 复制+贴图组合动作：对齐 ShareX AfterCaptureTasks
// CopyImageToClipboard | PinToScreen 位组合。不能拆成 copy/pin 两个
// 独立步骤：copy 步骤会把会话推进 Committing，pin 步骤的 Processing
// 守卫会误判"已取消"直接失败；组合整体做一次守卫+一次提交，底层复用
// 既有动作函数。
async fn run_copy_pin_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    begin_commit(session_id)?;
    let clipboard_id = copy_screenshot(stored).map_err(|e| e.to_string())?;
    emit_screenshot_history_update(app, clipboard_id).map_err(|e| e.to_string())?;
    let stored_for_pin = stored.clone();
    let pin_path = match tokio::task::spawn_blocking(move || prepare_pin_path(&stored_for_pin)).await {
        Ok(Ok(pin_path)) => pin_path,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(error) => return Err(format!("贴图文件准备线程失败: {error}")),
    };
    crate::windows::pin_image_window::pin_image_from_file(
        app.clone(),
        pin_path.to_string_lossy().to_string(),
        None, None, None, None, None, None, None, None, None, None, None,
    )
    .await
    .map_err(|e| format!("贴图失败: {e}"))?;
    Ok("已复制并贴图".to_string())
}

// 复制动作:对齐 ShareX CopyImageToClipboard——必须先 begin_commit 锁定
// 会话为 Committing 再写剪贴板,避免幽灵文本;成功通知主窗口历史刷新。
async fn run_copy_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    begin_commit(session_id)?;
    let clipboard_id = copy_screenshot(stored).map_err(|e| e.to_string())?;
    emit_screenshot_history_update(app, clipboard_id).map_err(|e| e.to_string())?;
    Ok("已复制到剪贴板".to_string())
}

// 保存动作:对齐 ShareX SaveImageToFileWithDialog——用户取消等同保存失败,
// 统一走失败清理避免会话卡在处理中;文件 IO 放线程池不占用异步运行时。
async fn run_save_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    let destination = match super::actions::choose_screenshot_save_destination(stored, app) {
        Ok(Some(destination)) => destination,
        Ok(None) => return Err("已取消保存截图".to_string()),
        Err(error) => return Err(error.to_string()),
    };
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    begin_commit(session_id)?;
    let stored = stored.clone();
    match tokio::task::spawn_blocking(move || save_screenshot(&stored, &destination)).await {
        Ok(Ok(())) => Ok("已保存截图".to_string()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(format!("保存截图线程失败: {error}")),
    }
}

// 贴图动作:对齐 ShareX PinToScreen——先 prepare 持久化文件,再提交会话
// 再建贴图窗口(贴图窗口依赖已提交的持久化文件);prepare 失败两条路径
// (文件错误/线程错误)统一报错。
async fn run_pin_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    let stored_for_pin = stored.clone();
    let pin_path = match tokio::task::spawn_blocking(move || prepare_pin_path(&stored_for_pin)).await {
        Ok(Ok(pin_path)) => pin_path,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(error) => return Err(format!("贴图文件准备线程失败: {error}")),
    };
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    begin_commit(session_id)?;
    crate::windows::pin_image_window::pin_image_from_file(
        app.clone(),
        pin_path.to_string_lossy().to_string(),
        None, None, None, None, None, None, None, None, None, None, None,
    )
    .await
    .map_err(|e| format!("贴图失败: {e}"))?;
    Ok("已贴图到屏幕".to_string())
}

// 编辑动作:对齐 ShareX 任务系统 DoAfterCaptureJobs 的 AnnotateImage——
// 打开图像编辑器窗口引导用户标注。编辑器是交互动作,完成后由编辑器
// 前端保存编辑结果并通过命令封闭编辑会话;此处只负责打开编辑器,
// 不推进会话提交(编辑前仍是 Processing,供编辑后继续其它动作)。
async fn run_edit_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    let image_path = stored.absolute_path.to_string_lossy().to_string();
    crate::windows::annotation::open_annotation_window(app, &image_path)?;
    Ok("已打开图像编辑器".to_string())
}

// AI 动作:对齐 ShareX DoOCR(任务系统的 OCR 位标志)——配置校验+云端发送
// 确认+识别+文本入剪贴板;识别失败且仍在 Processing 时保留会话供改用
// 其它动作(由调用方 is_total_failure 判定)。
async fn run_ai_action(
    app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    let settings = get_settings();
    if !settings.screenshot_ai_enabled {
        return Err("截图 AI 识别已关闭".to_string());
    }
    if super::ai_vision::validate_configuration(&settings.ai_api_key, &settings.ai_base_url, &settings.ai_model).is_err() {
        return Err("截图 AI 识别尚未完成配置".to_string());
    }
    if !settings.screenshot_ai_cloud_confirmed {
        let confirmed = tokio::task::spawn_blocking({
            let app_clone = app.clone();
            move || confirm_screenshot_ai_cloud_access(&app_clone)
        })
        .await
        .map_err(|error| format!("AI 确认对话框线程失败: {error}"))??;
        if !confirmed {
            return Err("已取消云端 AI 识别".to_string());
        }
    }
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    let result = recognize_image(
        &stored.absolute_path,
        &settings.ai_api_key,
        &settings.ai_base_url,
        &settings.ai_model,
        Some(&settings.screenshot_ai_prompt),
    )
    .await
    .map_err(|e| e.to_string());
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    match result {
        Ok(result) if !result.text.trim().is_empty() => {
            begin_commit(session_id)?;
            let clipboard_id = copy_screenshot_text(&result.text).map_err(|e| e.to_string())?;
            emit_screenshot_history_update(app, clipboard_id).map_err(|e| e.to_string())?;
            Ok("AI 识别文本已复制".to_string())
        }
        Ok(_) => Err("AI 未识别出文本".to_string()),
        Err(error) => Err(error),
    }
}

// 上传动作:对齐 ShareX AfterUploadTasks.CopyURLToClipboard——把截图产物
// 推送到已配置上传目标(默认 WebDAV),成功后把可访问 URL 复制进剪贴板。
// 目标未配置/上传失败走失败继续,不影响截图本身。
async fn run_upload_action(
    _app: &AppHandle,
    session_id: &str,
    stored: &StoredScreenshot,
    is_processing: &(dyn Fn(&str) -> bool + Sync),
    begin_commit: &(dyn Fn(&str) -> Result<(), String> + Sync),
) -> Result<String, String> {
    if !is_processing(session_id) {
        return Err("截图会话已取消".to_string());
    }
    let settings = get_settings();
    let target_id = settings.upload_target_id.clone();
    if target_id.is_empty() {
        return Err("未配置上传目标，请在设置中先选择上传方式".to_string());
    }
    let target = crate::services::upload::target_for(&target_id)?;
    // 文件读取放线程池,不占用异步运行时。
    let path = stored.absolute_path.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path)).await
        .map_err(|error| format!("读取产物线程失败: {error}"))?
        .map_err(|error| format!("读取产物文件失败: {error}"))?;
    let filename = format!("QC_{}_{}.png", stored.image_id, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0));
    let result = target.upload(&filename, bytes).await?;
    let url = result.url.clone();
    begin_commit(session_id)?;
    copy_screenshot_text(&url).map_err(|e| e.to_string())?;
    Ok(format!("已上传并复制链接: {}", result.url))
}

// AI 云端发送确认(对齐既有 confirm_screenshot_ai_cloud_access)
fn confirm_screenshot_ai_cloud_access(app: &AppHandle) -> Result<bool, String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

    let settings = get_settings();
    if settings.screenshot_ai_cloud_confirmed {
        return Ok(true);
    }

    let message = if settings.language.starts_with("zh") {
        "AI 识别会将当前截图选区发送至你配置的 AI 服务进行处理。图片不会使用本地 OCR 静默替代。\n\n是否继续？"
    } else {
        "AI recognition sends the current screenshot selection to your configured AI service. It will not silently fall back to local OCR.\n\nContinue?"
    };
    if !app
        .dialog()
        .message(message)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
    {
        return Ok(false);
    }

    crate::services::settings::update_with(|settings| settings.screenshot_ai_cloud_confirmed = true)
        .map_err(|error| format!("保存截图 AI 隐私确认失败: {error}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/services/screenshot/workflow.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取动作链源码失败")
    }

    fn prod_source() -> String {
        source()
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(&source())
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 动作链必须"失败继续+汇总"(对齐 ShareX DoAfterCaptureJobs 各动作
    // try/catch 不中断):遍历步骤、每步失败推入 failed 并继续。可反证
    // (删循环/改 early return FAILED)。
    #[test]
    fn workflow_continues_on_step_failure_and_collects_results() {
        let src = prod_source();
        let start = src
            .find("pub async fn execute_workflow")
            .expect("缺 execute_workflow");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        assert!(
            body.contains("result.succeeded.push(summary)"),
            "每步成功必须推入 succeeded 汇总"
        );
        assert!(
            body.contains("result.failed.push((step.action.clone(), error))"),
            "每步失败必须推入 failed 汇总并继续"
        );
        let for_pos = body.find("for step in steps").expect("缺遍历循环");
        let after_for = &body[for_pos..];
        assert!(
            !after_for.contains("break;"),
            "单步失败不得 break 中断后续步骤"
        );
    }

    // 汇总结构必须含成功/失败列表与完全失败判定、错误汇总文本
    // (对齐 ShareX Result.IsError + ErrorsToString)
    #[test]
    fn workflow_result_has_summary_structure() {
        let src = source();
        assert!(
            src.contains("pub struct WorkflowActionResult"),
            "必须提供动作链汇总结构"
        );
        assert!(
            src.contains("pub succeeded: Vec<String>"),
            "汇总必须含成功列表"
        );
        assert!(
            src.contains("pub failed: Vec<(String, String)>"),
            "汇总必须含失败列表(动作, 错误)"
        );
        assert!(
            src.contains("pub fn is_total_failure"),
            "必须提供完全失败判定"
        );
        assert!(
            src.contains("pub fn error_summary"),
            "必须提供错误汇总文本(对齐 ErrorsToString)"
        );
    }

    // 复制动作必须先提交会话再写剪贴板(既有 copy_action_commits_before
    // _clipboard_write 语义收编进引擎后保留)
    #[test]
    fn copy_action_commits_before_clipboard_write() {
        let src = prod_source();
        let start = src
            .find("async fn run_copy_action")
            .expect("缺 run_copy_action");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        let commit = body
            .find("begin_commit(session_id)?")
            .expect("缺少会话提交");
        let copy = body
            .find("copy_screenshot(stored)")
            .expect("缺少剪贴板写入");
        assert!(
            commit < copy,
            "必须先提交会话再写剪贴板(避免幽灵文本)"
        );
        assert!(
            body.contains("emit_screenshot_history_update(app, clipboard_id)"),
            "复制成功必须通知历史刷新"
        );
    }

    // 贴图动作 prepare 失败两条路径必须报错且 commit 先于建窗
    #[test]
    fn pin_action_commits_before_window_creation_and_prepare_failures_report() {
        let src = prod_source();
        let start = src
            .find("async fn run_pin_action")
            .expect("缺 run_pin_action");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        let commit = body
            .find("begin_commit(session_id)?")
            .expect("缺少会话提交");
        let pin_window = body
            .find("pin_image_from_file(")
            .expect("缺少贴图窗口创建");
        assert!(
            commit < pin_window,
            "必须先提交会话再创建贴图窗口"
        );
        assert!(
            body.contains("prepare_pin_path(&stored_for_pin)"),
            "贴图必须先 prepare 持久化文件"
        );
        assert!(
            body.contains("贴图文件准备线程失败"),
            "prepare 线程失败必须报错"
        );
    }

    // AI 动作:配置校验+云端确认先于识别请求
    #[test]
    fn ai_action_validates_and_confirms_before_request() {        let src = prod_source();
        let start = src
            .find("async fn run_ai_action")
            .expect("缺 run_ai_action");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        let config = body
            .find("validate_configuration(")
            .expect("AI 动作缺少可用性校验");
        let confirm = body
            .find("confirm_screenshot_ai_cloud_access")
            .expect("AI 动作缺少云端发送确认");
        let request = body
            .find("recognize_image(")
            .expect("AI 动作缺少识别请求");
        assert!(
            config < confirm && confirm < request,
            "必须先校验配置、确认云端发送，再发起 AI 请求"
        );
    }

    // 组合预设 copy+pin:复制后立即贴图,共享一次会话提交,先复制再贴图;
    // 若拆成独立步骤,复制步骤推进 Committing 后贴图步骤的 Processing 守卫
    // 会误判"已取消"直接失败(状态机不允许 Committing 再 Processing)。
    #[test]
    fn copy_pin_combo_shares_single_commit_and_runs_copy_before_pin() {
        let src = prod_source();
        let start = src
            .find("\"copy+pin\" => {")
            .expect("缺 copy+pin 组合预设");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];

        // 必须整体一次会话提交(不能拆步骤,否则贴图误判已取消)。
        let commit = body
            .find("begin_commit(session_id)?;")
            .expect("组合预设必须整体提交一次");
        let copy_call = body
            .find("copy_screenshot(stored)")
            .expect("组合预设必须先执行复制");
        let pin_call = body
            .find("pin_image_from_file(")
            .expect("组合预设必须随后执行贴图");
        assert!(
            commit < copy_call && copy_call < pin_call,
            "必须先整体提交,再复制,最后贴图"
        );
        assert!(
            body.contains("prepare_pin_path(&stored_for_pin)"),
            "贴图必须先 prepare 持久化文件"
        );
        assert!(
            body.contains("已复制并贴图"),
            "组合成功必须合并两动作摘要"
        );
        assert!(
            body.contains("is_processing(session_id)"),
            "组合预设必须做会话处理中守卫"
        );
    }

    // 上传动作:把产物读入内存(线程池)→ 推到已配置目标 → 成功后复制
    // 可访问 URL 进剪贴板(对齐 ShareX AfterUploadTasks.CopyURLToClipboard)。
    #[test]
    fn upload_action_reads_blocking_and_copies_result_url() {
        let src = prod_source();
        let start = src
            .find("async fn run_upload_action")
            .expect("缺上传动作");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        assert!(
            body.contains("upload_target_id"),
            "上传动作必须读取已配置目标 id"
        );
        assert!(
            body.contains("target_for(&target_id)"),
            "上传动作必须走目标派发"
        );
        assert!(
            body.contains("spawn_blocking(move || std::fs::read(&path))"),
            "产物读盘必须走线程池"
        );
        assert!(
            body.contains("target.upload(&filename, bytes)"),
            "上传动作必须调用目标 upload"
        );
        let commit = body.find("begin_commit(session_id)?;")
            .expect("上传动作必须提交会话");
        let copy = body.find("copy_screenshot_text(&url)")
            .expect("上传成功后必须复制可访问 URL");
        assert!(commit < copy, "必须先提交会话再复制链接");
    }

    // 保存动作:用户取消=失败(对齐 ShareX SaveImageToFileWithDialog 取消
    // 即放弃保存),文件 IO 必须走线程池不占异步运行时;提交先于写盘。
    #[test]
    fn save_action_moves_file_io_to_blocking_and_treats_cancel_as_failure() {
        let src = prod_source();
        let start = src
            .find("async fn run_save_action")
            .expect("缺 run_save_action");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        let dialog = body
            .find("choose_screenshot_save_destination(stored, app)")
            .expect("保存动作缺少保存对话框");
        let cancel = body
            .find("Ok(None) => return Err(\"已取消保存截图\".to_string())")
            .expect("保存取消必须视为失败");
        assert!(dialog < cancel, "必须先弹出保存对话框再判定取消");
        assert!(
            body.contains("spawn_blocking(move || save_screenshot(&stored, &destination))"),
            "文件 IO 必须走线程池"
        );
        assert!(body.contains("保存截图线程失败"), "线程失败必须报错");
        let commit = body
            .find("begin_commit(session_id)?;")
            .expect("缺少会话提交");
        let save = body
            .find("save_screenshot(&stored, &destination)")
            .expect("缺少文件保存");
        assert!(commit < save, "必须先提交会话再写盘");
    }
}
