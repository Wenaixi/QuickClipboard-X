// 选区调整撤销历史（ShareX 公开行为参考：选区移动/调整后可撤销误操作）。
// 历史保存每次调整前的选区状态，push 追加、undo 弹出最近一条，上限默认 10 条。

function assertHistoryArray(history) {
  if (!Array.isArray(history)) {
    throw new TypeError('历史必须是数组');
  }
}

function assertLimit(limit) {
  if (!Number.isInteger(limit) || limit <= 0) {
    throw new RangeError('上限必须是正整数');
  }
}

// 追加一个调整前的选区状态，返回新数组（不可变），超过上限时丢弃最旧条目。
export function pushSelectionHistory(history, selection, limit = 10) {
  assertHistoryArray(history);
  assertLimit(limit);
  const next = [...history, selection];
  return next.length > limit ? next.slice(next.length - limit) : next;
}

// 撤销最近一次调整：弹出历史末尾状态返回给调用方，无历史（含空数组或未初始化）返回 null。
export function undoSelectionHistory(history) {
  if (history === null || history === undefined) {
    return null;
  }
  assertHistoryArray(history);
  if (history.length === 0) {
    return null;
  }
  return { history: history.slice(0, -1), selection: history[history.length - 1] };
}

// 是否存在可撤销的调整记录。
export function canUndoSelection(history) {
  return Array.isArray(history) && history.length > 0;
}
