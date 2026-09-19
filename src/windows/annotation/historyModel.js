// R3 图像编辑器撤销重做栈（深度参考 ShareX.ImageEditor EditorHistory/
// EditorMemento 双栈 + 代际令牌防异步覆盖）。纯函数无 DOM 依赖：
// pushSnapshot 压入编辑前快照（内容深拷贝防共享引用污染），undo 弹出
// 最近快照并放入重做栈，redo 从重做栈恢复；代际递增使旧异步回调的
// push 静默丢弃（与 preview 窗口 PREVIEW_DESTROY_TIMER_VERSION 同款）。

function assertSnapshotArray(history) {
  if (!Array.isArray(history)) {
    throw new TypeError('历史必须是数组');
  }
}

function assertLimit(limit) {
  if (!Number.isInteger(limit) || limit <= 0) {
    throw new RangeError('上限必须是正整数');
  }
}

// 快照深拷贝：标注层数组逐项展开（图层含 shape/params/points 数组，
// 防止 undo 弹出后修改污染栈内旧状态）。
function deepCloneSnapshot(snapshot) {
  if (snapshot === null || typeof snapshot !== 'object') {
    return snapshot;
  }
  if (Array.isArray(snapshot)) {
    return snapshot.map(deepCloneSnapshot);
  }
  return Object.fromEntries(Object.entries(snapshot).map(([key, value]) => [key, deepCloneSnapshot(value)]));
}

// 追加编辑前快照（不可变，超过上限丢弃最旧），同时清空重做栈（新分支
// 使已撤销动作不可重做——对齐 ShareX 编辑后清空 redo 栈语义）。
export function pushSnapshot(history, snapshot, limit = 50, redoStack = []) {
  assertSnapshotArray(history);
  assertLimit(limit);
  const next = [...history, deepCloneSnapshot(snapshot)];
  const trimmed = next.length > limit ? next.slice(next.length - limit) : next;
  return { history: trimmed, redoStack: [] };
}

// 撤销：弹出最近快照放入重做栈，返回给调用方；无历史返回 null。
export function undoSnapshot(history, redoStack = []) {
  assertSnapshotArray(history);
  if (history.length === 0) {
    return null;
  }
  const current = history[history.length - 1];
  return {
    history: history.slice(0, -1),
    redoStack: [...redoStack, deepCloneSnapshot(current)],
    snapshot: deepCloneSnapshot(current),
  };
}

// 重做：从重做栈弹出最近快照恢复到历史；无重做返回 null。
export function redoSnapshot(history, redoStack = []) {
  assertSnapshotArray(redoStack);
  if (redoStack.length === 0) {
    return null;
  }
  const next = redoStack[redoStack.length - 1];
  return {
    history: [...history, deepCloneSnapshot(next)],
    redoStack: redoStack.slice(0, -1),
    snapshot: deepCloneSnapshot(next),
  };
}

export function canUndoSnapshots(history) {
  return Array.isArray(history) && history.length > 0;
}

export function canRedoSnapshots(redoStack) {
  return Array.isArray(redoStack) && redoStack.length > 0;
}

// 代际令牌：递增使旧异步回调失效（防止画布异步操作乱序覆盖快照栈）。
export function advanceSnapshotGeneration(generationRef) {
  generationRef.current += 1;
  return generationRef.current;
}

export function isCurrentSnapshotGeneration(expected, generationRef) {
  return expected === generationRef.current;
}