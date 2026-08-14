# Windows 高性能截图模块实施方案

## 1. 目标

在开发主分支 `mine` 自研 Windows 截图模块，替代上游已删除的私有 `screenshot-suite`。第一期仅支持 Windows，提供单显示器矩形选区及四种动作：复制、另存为、贴图、AI 视觉识别。

截图本地交互与屏幕捕获优先保证流畅。AI 只在用户明确选择“AI 识别”后异步调用，绝不参与鼠标拖拽、选区绘制或截图首帧链路。

## 2. 首期范围

### 纳入

- 全局快捷键、主窗口更多菜单、托盘菜单启动截图。
- 当前鼠标所在显示器内的矩形选区。
- 混合 DPI、负坐标副屏和 4K 显示器正确换算。
- 复制到系统剪贴板并进入图片历史。
- 另存为 PNG。
- 使用现有 Tauri `pin_image_window` 贴图。
- 通过云端多模态大模型识别图片文字。
- 设置页中的截图开关、截图快捷键和 AI 配置状态提示。

### 不纳入

- 跨显示器选区。
- 滚动截图、窗口或控件识别、录屏。
- 箭头、画笔、文字、马赛克等标注编辑。
- 将本地 `qcocr` 作为截图 AI 识别的回退。
- 恢复 `screenshot-suite`、`gpu-image-viewer` 或私有依赖。

单显示器边界用于优先保证混合 DPI 和多显示器负坐标下的正确性。跨屏选区应在本期实机验证和 DPI 自动化通过后独立设计。

## 3. 架构

新增模块：

- `src-tauri/src/services/screenshot/session.rs`：单会话状态、会话 ID、取消令牌、资源清理。
- `src-tauri/src/services/screenshot/capture/windows.rs`：Windows 原生捕获后端。
- `src-tauri/src/services/screenshot/image_store.rs`：选区图片编码、内容寻址和临时资源管理。
- `src-tauri/src/services/screenshot/ai_vision.rs`：多模态 AI 请求、图片限制、响应解析和错误脱敏。
- `src-tauri/src/windows/screenshot_window/mod.rs`：选区覆盖窗创建、复用、销毁与显示器几何。
- `src-tauri/src/commands/screenshot.rs`：Tauri 命令边界。
- `src/windows/screenshot/`：选区交互页面与纯前端几何逻辑。

会话状态只能按以下顺序流转：

`Idle -> Selecting -> Processing -> Idle`

每个会话保存随机 `sessionId`、目标显示器物理矩形、主窗口启动前可见状态、覆盖窗状态和临时文件所有权。

不变量：

1. 同一时间只能有一个截图会话。
2. 重复触发只聚焦当前会话，不能新开覆盖窗。
3. 前端完成或取消必须带 `sessionId`，过期页面不能影响新会话。
4. 失败、取消、显示器移除和应用退出必须释放资源并回到 `Idle`。
5. 只恢复本会话主动隐藏过的主窗口，不覆盖用户中途手动做出的显隐操作。

## 4. 高性能捕获路线

Windows 使用 `Windows.Graphics.Capture` 与 D3D11：

- 用 `IGraphicsCaptureItemInterop::CreateForMonitor` 建立目标显示器捕获项。
- 用 `Direct3D11CaptureFramePool::CreateFreeThreaded` 建立独立于 UI 线程的帧池。
- 用户确认选区后才创建一次性捕获会话；拖拽选区时绝不捕获桌面。
- 通过 GPU `CopySubresourceRegion` 只复制选区；不将整屏传回 CPU。
- PNG 编码、哈希、磁盘写入和 AI 请求均在后台任务执行。

实现前应完成最小 Windows 原型，核实现有 `windows` crate 的精确 feature、首帧延迟、混合 DPI 和显示器热插拔行为。官方参考：

- [Microsoft 屏幕捕获概览](https://learn.microsoft.com/zh-cn/windows/apps/develop/media-authoring-processing/screen-capture)
- [CreateFreeThreaded](https://learn.microsoft.com/zh-tw/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded)
- [CreateForMonitor](https://learn.microsoft.com/en-za/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createformonitor)

性能约束：

| 指标 | 目标 |
| --- | --- |
| 1080p 覆盖窗启动 p95 | 不超过 200ms |
| 确认选区至开始捕获 p95 | 不超过 250ms |
| 拖拽选区 | 无可见掉帧，目标 60 FPS |
| 拖拽路径 | 不做截图、编码、磁盘 IO、数据库 IO、AI 或网络请求 |

调试模式记录覆盖窗启动、确认至首帧、GPU 读回、PNG 编码、动作完成耗时。4K 编码和 AI 耗时单独统计，不纳入交互流畅度指标。

## 5. 选区覆盖窗

新页面包含 `index.html`、`index.jsx`、`App.jsx`、`selectionModel.js` 与 `screenshot.css`。

- 覆盖窗大小固定为当前显示器物理矩形，透明、无边框、置顶、不在任务栏显示。
- 用四块 CSS 遮罩形成透明选区，不把桌面截图画进 Canvas。
- `pointermove` 只更新 `ref` 中的几何；每帧最多一次 `requestAnimationFrame` 写入 CSS 变量。
- 不允许每次鼠标移动调用 React state、Tauri 命令或创建图片对象。
- 支持反向拖拽归一化、边界 clamp、最小 1x1 像素、Esc、右键和失焦取消。
- 有效选区出现后显示轻量工具栏：复制、另存为、贴图、AI 识别；默认复制，Enter 确认。
- 提交时将 CSS 像素乘以 `devicePixelRatio`，转换为目标显示器内的物理像素。

新增最小 `screenshot` capability，只授予事件、窗口和保存对话框所需权限；不恢复旧的全盘 `fs:scope` 或 `screenshot-suite:default` 权限。

## 6. 图片交付

所有动作共用同一份已裁切 PNG，避免重复捕获与重复编码。

### 复制

1. PNG 以内容哈希保存到 `clipboard_images/<sha256前16位>.png`。
2. 复用现有图片剪贴板写入能力。
3. 新增受限“自产截图入库”入口，复用既有图片历史、SQLite 和 `clipboard-updated` 事件。
4. 写系统剪贴板前设置监听抑制与内容哈希缓存，确保监听器不会生成第二条记录。

### 另存为

复用保存对话框选择目标路径，后台原子复制已编码 PNG。用户取消不报错、不写历史。

### 贴图

将 PNG 写入受控 `pin_images` 路径，调用 `windows::pin_image_window::pin_image_from_file`，不恢复原生贴图引擎。

## 7. AI 视觉识别

### 7.1 决策

截图后的文字识别固定使用云端多模态大模型，不走本地 `qcocr`。模型调用只在用户选择“AI 识别”后进行，由 Rust 后端执行。这样既利用视觉模型对复杂排版、多语言、表格和上下文的能力，也不会影响截图的本地流畅度。

项目已有 `aiApiKey`、`aiBaseUrl`、`aiModel` 配置。截图 AI 识别复用这些共享配置，但必须验证所选模型支持图片输入；不能假定现有纯文本翻译模型具备视觉能力。

### 7.2 请求链路

`用户选择 AI 识别 -> 裁切 PNG -> 后台压缩或缩放 -> Rust 读取共享 AI 配置 -> HTTPS 调用多模态模型 -> 解析结构化结果 -> 写入文本剪贴板与文本历史`

前端不得读取、保存或发送 API Key。`services/screenshot/ai_vision.rs` 是唯一允许构造 AI 请求的位置。

### 7.3 接口与输出契约

第一期使用 OpenAI 兼容的多模态 `chat/completions` 接口：

- 请求同时带固定 OCR 指令和 `data:image/...;base64` 图片内容。
- 默认非流式，避免截图工具栏维护不稳定的增量状态。
- 要求模型只返回以下 JSON：

`{ "text": "完整识别文本", "blocks": [{ "type": "paragraph", "text": "段落文本" }] }`

第一期只使用 `text`，`blocks` 为后续表格、布局和高亮预留。模型输出不是 JSON 时只允许一次受限提取；仍无法解析则返回明确错误，不能把不可信内容静默写入剪贴板。

固定提示词必须要求：完整转写、保留段落和换行逻辑、不翻译、不总结、不补充原图没有的信息、无法辨认处用 `[无法辨认]` 标记。

### 7.4 隐私、限制与错误

- 图片上传前必须限制最大边长和请求总字节数；实现时设定合理默认值与硬上限。
- 先使用 PNG；超阈值时后台转 JPEG，以文字可读性为优先。
- 首次使用 AI 识别时明确提示：选区图片会发送至用户配置的 AI 服务。
- 未配置 API Key、Base URL、模型，或确认模型不支持视觉时，AI 动作禁用并提供配置入口。
- 日志、通知和错误信息不得包含 API Key、认证头、完整 data URL 或原图文本。
- 必须区分网络失败、认证失败、限流、模型不支持视觉、响应格式错误和服务端错误。
- 用户取消会话后，未发起的请求不得开始；已发起请求的结果必须丢弃，不能覆盖剪贴板。
- AI 失败时保留本地截图，让用户仍可选择复制、保存或贴图。

### 7.5 设置

新增最小字段：

- `screenshotAiEnabled`：是否显示 AI 识别动作。
- `screenshotAiPrompt`：可选高级提示词，空值时使用内置安全默认提示词。

不复制保存 API Key；继续共用 `aiApiKey`、`aiBaseUrl` 和 `aiModel`。新增真实 `test_screenshot_ai_config` 后端命令，验证视觉能力或进行轻量请求；禁止使用仅靠计时器模拟成功的配置测试。

## 8. 设置、快捷键与入口

在 Rust `AppSettings`、前端 `defaultSettings`、保存与导入链路统一新增：

- `screenshotEnabled: true`
- `screenshotShortcut: 'Ctrl+Shift+A'`
- `screenshotAiEnabled: true`
- `screenshotAiPrompt: ''`

在 `services/system/hotkey/global.rs` 以 backend ID `screenshot` 注册快捷键，遵循 `mine` 已有锁序、前台应用过滤、低内存模式、错误状态和失败清理。

设置页新增截图区：截图开关、截图快捷键、AI 识别开关、当前视觉配置状态与跳转 AI 配置入口。标题栏和托盘各提供“截屏”，但都调用同一 `start_screenshot` 命令。

## 9. 实施与验证

1. 将 `main` 合并到 `mine`，清除旧私有截图残留。
2. 实现会话状态机与 Windows.Graphics.Capture 原型，测试状态、坐标、取消和 cleanup。
3. 实现覆盖窗，使用 Node 原生 `node:test` 覆盖选区几何、DPR、取消和 RAF 批处理。
4. 实现复制、保存和贴图，测试哈希去重、原子写、剪贴板监听去重和贴图参数。
5. 实现 AI 视觉服务，使用 mock HTTP 服务测试请求构造、尺寸限制、JSON 解析、超时、取消和错误脱敏；测试不得调用真实模型或真实网络。
6. 实现设置、快捷键和菜单，测试 serde 默认值、快捷键状态、国际化 key 和 Vite 构建。
7. 实机验证 100%/125%/150% DPI、主副屏负坐标、4K、显示器移除、四种动作、快捷键冲突、AI 成功、认证失败、限流及非视觉模型。

每个独立模块完成后立即提交。每次提交前运行受影响测试；阶段完成时运行截图前端测试、`npm run build`、`cargo test --lib --no-default-features` 和相关 Rust 格式检查。