// R2 截图新能力：任意多边形路径的纯函数模型（ShareX RegionCaptureTasks
// 的 RegionPolygon 参考——点击锚点 + 闭合 + 路径转裁剪）。全部纯函数，
// 无 DOM 依赖，供 test:screenshot 覆盖：锚点点击/闭合判定/路径→物理
// 像素顶点/多边形命中测试/包围盒。

function assertPoints(points) {
  if (!Array.isArray(points)) {
    throw new TypeError('顶点数组缺失');
  }
  for (const point of points) {
    // 顶点可能是 {x,y}(锚点集/命中测试)或 {left,top}(多边形路径存储,
    // 与 CSS --selection-polygon 同构);两者取其一校验即可,缺 x/y 时
    // 回退读 left/top,否则空对象/错误形状会被误判为合法顶点。
    const px = point && point.x !== undefined ? point.x : point && point.left;
    const py = point && point.y !== undefined ? point.y : point && point.top;
    if (!point || !Number.isFinite(px) || !Number.isFinite(py)) {
      throw new TypeError('顶点坐标必须是有限数字');
    }
  }
}

function assertBounds(bounds) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
}

// 点与已有点的距离：闭合判定阈值为当点击靠近首锚点（含容差）时闭合。
export function distanceTo(point, other) {
  if (!point || !Number.isFinite(point.x) || !Number.isFinite(point.y)) {
    throw new TypeError('点坐标必须是有限数字');
  }
  if (!other || !Number.isFinite(other.x) || !Number.isFinite(other.y)) {
    throw new TypeError('另一端点坐标必须是有限数字');
  }
  return Math.hypot(point.x - other.x, point.y - other.y);
}

// 判定点击是否闭合多边形（靠近首锚点或双击）：容差默认 8px（ShareX 锚点
// 闭合容差参考）。points 至少有一个锚点（首锚点），返回是否闭合。
export function shouldClosePolygon(point, points, tolerance = 8) {
  assertPoints(points);
  if (points.length === 0) {
    return false;
  }
  if (!Number.isFinite(tolerance) || tolerance < 0) {
    throw new RangeError('闭合容差不能为负数');
  }
  return distanceTo(point, points[0]) <= tolerance;
}

// 判定闭合：双击属于显式闭合（与靠近首锚点同义）。
export function isPolygonClosed(points) {
  assertPoints(points);
  return points.length >= 3;
}

// 多边形路径 → 物理像素顶点数组（供后端 CaptureRect::from_polygon_bounds
// 取包围盒捕获）：逻辑坐标按 devicePixelRatio 缩放，Double 取整与选区
// 边界口径一致（floor 左上/ceil 右下）。调用方 polygonPath 元素为
// {left, top}（与 CSS --selection-polygon 同构），须按 left/top 读取——
// 读 x/y 会拿到 undefined → NaN 顶点，后端包围盒捕获失效。
export function polygonPhysicalVertices(points, devicePixelRatio) {
  assertPoints(points);
  if (!Number.isFinite(devicePixelRatio) || devicePixelRatio <= 0) {
    throw new RangeError('devicePixelRatio 必须为正数');
  }
  if (points.length < 3) {
    throw new RangeError('多边形至少需要 3 个不共线顶点');
  }
  return points.map((point) => ({
    x: Math.floor((point.left ?? point.x) * devicePixelRatio),
    y: Math.floor((point.top ?? point.y) * devicePixelRatio),
  }));
}

// 射线法命中测试（奇数交叉规则）：点是否在多边形内部。闭合判定不依赖
// 首尾是否显式重合（隐式闭合：首尾不必落在同一坐标）。
export function pointInPolygon(point, points) {
  assertPoints(points);
  if (points.length < 3) {
    return false;
  }
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i, i += 1) {
    const current = points[i];
    const previous = points[j];
    const intersects = (current.y > point.y) !== (previous.y > point.y)
      && point.x < ((previous.x - current.x) * (point.y - current.y)) / (previous.y - current.y) + current.x;
    if (intersects) inside = !inside;
  }
  return inside;
}

// 多边形轴对齐包围盒：{ left, top, width, height, right, bottom }，
// 返回 null 当顶点不足或退化（无面积——含全部顶点共线：三点横跨
// 对角时 right/left 与 bottom/top 都有跨度但实际无面积）。
export function polygonBounds(points) {
  assertPoints(points);
  if (points.length < 3) {
    return null;
  }
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const left = Math.min(...xs);
  const top = Math.min(...ys);
  const right = Math.max(...xs);
  const bottom = Math.max(...ys);
  if (right <= left || bottom <= top) {
    return null;
  }
  // 面积退化：全部顶点共线（叉积恒为零）时无面，返回 null 供调用方
  // 回退矩形选区（对齐后端 CaptureRect::from_polygon_bounds 退化拒绝）。
  const first = points[0];
  const second = points[1];
  for (let i = 2; i < points.length; i += 1) {
    const p = points[i];
    const cross = (second.x - first.x) * (p.y - first.y) - (second.y - first.y) * (p.x - first.x);
    if (cross !== 0) {
      return { left, top, right, bottom, width: right - left, height: bottom - top };
    }
  }
  return null;
}

export function polygonPathPoints(points) {
  assertPoints(points);
  return points.map((point) => ({ x: point.x, y: point.y }));
}