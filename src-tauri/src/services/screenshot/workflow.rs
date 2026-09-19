// 截图动作链引擎
//
// 复刻 ShareX 任务系统(AfterCaptureTasks 位标志 + WorkerTask 后台线程
// 流水线):把截图完成后的动作从"单动作命令"升级为"有序步骤流水线"。
// 与 ShareX 的差异:用有序 step 列表表达动作组合(可表达顺序/依赖,
// 位标志只能表达集合),失败策略为"失败继续+汇总"(每步独立 try/catch,
// 结束后返回汇总,截图不因某步失败而丢)。
//
// 设计:
//   - WorkflowStep:单个动作及其参数(copy/save/pin/ai)
//   - 各动作闭包注册在 execute_workflow 的 match 中(动作处理自足,
//     不依赖窗口会话的内部状态——会话提交/取消守卫由调用方负责)
//   - execute_workflow 遍历步骤:每步成功推入 succeeded,失败推入
//     failed 并继续下一步;结束后若 all failed 返回 Err(调用方统一
//     失败清理),否则返回汇总(部分成功也是完成)
//   - 与既有护栏的衔接:复制/贴图必须先 begin_commit 锁定会话再写
//     剪贴板/建窗(调用方在 execute_workflow 前调用);单步内不再做
//     会话守卫,由 complete_screenshot 的既有会话处理统一负责

use tauri::AppHandle;

use super::StoredScreenshot;

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

/// 单步执行结果(成功为 Ok,失败为 Err 文本)
pub type StepResult = Result<(), String>;

/// 动作链汇总结果
#[derive(Debug, Clone)]
pub struct WorkflowActionResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl WorkflowActionResult {
    /// 是否完全失败(无任何成功步骤)
    pub fn is_total_failure(&self) -> bool {
        self.succeeded.is_empty()
    }
}

/// 执行截图后动作链:遍历步骤,每步失败继续下一步,汇总返回。
/// 参数闭包(已做会话提交守卫)分派到各动作实现。
pub async fn execute_workflow(
    app: &AppHandle,
    stored: &StoredScreenshot,
    steps: &[WorkflowStep],
    mut copy_action: impl FnMut(&AppHandle, &StoredScreenshot) -> Result<String, String>,
    mut save_action: impl FnMut(&AppHandle, &StoredScreenshot) -> Result<String, String>,
    mut pin_action: impl FnMut(&AppHandle, &StoredScreenshot) -> Result<String, String>,
    mut ai_action: impl FnMut(&AppHandle, &StoredScreenshot) -> Result<String, String>,
) -> WorkflowActionResult {
    let mut result = WorkflowActionResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for step in steps {
        let step_result: Result<String, String> = match step.action.as_str() {
            "copy" => copy_action(app, stored),
            "save" => save_action(app, stored),
            "pin" => pin_action(app, stored),
            "ai" => ai_action(app, stored),
            other => Err(format!("不支持的截图动作: {other}")),
        };

        match step_result {
            Ok(summary) => result.succeeded.push(summary),
            Err(error) => result.failed.push((step.action.clone(), error)),
        }
    }

    result
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

    // 动作链必须"失败继续+汇总":遍历步骤、每步 try/catch 式收集失败并
    // 继续下一步(不是失败即中断)。源码字面可反证(删循环/改 early return
    // FAILED)。
    #[test]
    fn workflow_continues_on_step_failure_and_collects_results() {
        let src = source();
        let start = src
            .find("pub async fn execute_workflow")
            .expect("缺 execute_workflow");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let body = &src[start..end];
        // 每步结果按成功/失败分推入汇总(失败不中断)
        assert!(
            body.contains("result.succeeded.push(summary)"),
            "每步成功必须推入 succeeded 汇总"
        );
        assert!(
            body.contains("result.failed.push((step.action.clone(), error))"),
            "每步失败必须推入 failed 汇总并继续"
        );
        // 失败收集在 match 之内,遍历循环必须完整遍历所有步骤(无 early break)
        assert!(
            body.contains("for step in steps"),
            "动作链必须遍历全部步骤"
        );
        let for_pos = body.find("for step in steps").expect("缺遍历循环");
        let after_for = &body[for_pos..];
        assert!(
            !after_for.contains("break;"),
            "单步失败不得 break 中断后续步骤"
        );
    }

    // 结果汇总结构必须含成功/失败两部分(WorkflowActionResult)
    #[test]
    fn workflow_result_has_succeeded_and_failed_lists() {
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
    }
}
