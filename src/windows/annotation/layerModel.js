// 编辑器画布标注层模型：深度参考 ShareX.ImageEditor 的 BaseShape 层语义
//（Annotation 类型 + 绘制参数 + 顶点/坐标数组）。全部纯函数无 DOM
// 依赖：addLayer 追加图层（含代际校验）、updateLayer 原位更新、
// removeLayer/clearLayers 删除、reorderLayer 调整 z 序、mergeLayer 扁平化
// 栅格标注为单层（对齐 ShareX FlattenAnnotations）。

export const LAYER_TYPES = ['rect', 'ellipse', 'line', 'arrow', 'pen', 'text', 'highlight', 'crop', 'rotate', 'flip'];

// 新建标注层：id 调用方注入（后续代际校验防异步乱序）。
export function createLayer(id, type, params = {}, points = []) {
  if (!LAYER_TYPES.includes(type)) {
    throw new TypeError(`未知标注类型: ${type}`);
  }
  if (typeof id !== 'string' || id.length === 0) {
    throw new TypeError('图层 id 必须是非空字符串');
  }
  return {
    id,
    type,
    params: { ...params },
    points: Array.isArray(points) ? points.map((point) => ({ ...point })) : [],
  };
}

export function addLayer(layers, layer, expectedGeneration, generationRef) {
  if (expectedGeneration !== generationRef.current) {
    throw new Error('画布代际已过期');
  }
  return [...layers, layer];
}

export function updateLayer(layers, id, patch) {
  return layers.map((layer) => (
    layer.id === id ? { ...layer, params: { ...layer.params, ...(patch.params || {}) }, points: patch.points ? [...patch.points] : layer.points } : layer
  ));
}

export function removeLayer(layers, id) {
  return layers.filter((layer) => layer.id !== id);
}

export function clearLayers() {
  return [];
}

// 调整 z 序：把 id 移动到 toIndex（0 为最底）。sharex z-order 语义：
// 后绘制的在上层，调整后新数组不可变。
export function reorderLayer(layers, id, toIndex) {
  const index = layers.findIndex((layer) => layer.id === id);
  if (index === -1) {
    return layers;
  }
  const next = layers.filter((layer) => layer.id !== id);
  const clamped = Math.max(0, Math.min(toIndex, next.length));
  next.splice(clamped, 0, layers[index]);
  return next;
}

// 栅格化扁平标注为单层：canvas 已把标注画进位图，此后只保留一个
// flat 层（对齐 ShareX FlattenAnnotations）。
export function mergeLayers() {
  return [{ id: 'flat', type: 'flat', params: {}, points: [] }];
}