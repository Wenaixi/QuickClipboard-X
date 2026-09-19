// 编辑器画布 render 指令模型：把标注层参数渲染成 canvas 2D 指令集合
//（全部纯函数，无 DOM；ShareX.ImageEditor GraphicsPath/Annotation.Draw
// 的简化对齐——各图层类型只生成渲染参数，canvas 由调用方执行）。
// clip 指令（crop 多边形/选区裁剪）在输出编码前由编辑器前端按指令
// 提交到后端像素操作。

export const RENDER_COMMANDS = {
  rect: (params) => ({ type: 'rect', x: params.x, y: params.y, w: params.w, h: params.h, stroke: params.stroke, fill: params.fill }),
  ellipse: (params) => ({ type: 'ellipse', x: params.x, y: params.y, w: params.w, h: params.h, stroke: params.stroke, fill: params.fill }),
  line: (params) => ({ type: 'line', x1: params.x1, y1: params.y1, x2: params.x2, y2: params.y2, stroke: params.stroke, width: params.width }),
  arrow: (params) => ({ type: 'arrow', x1: params.x1, y1: params.y1, x2: params.x2, y2: params.y2, stroke: params.stroke, width: params.width }),
  pen: (params) => ({ type: 'polyline', points: params.points, stroke: params.stroke, width: params.width }),
  text: (params) => ({ type: 'text', x: params.x, y: params.y, text: params.text, fill: params.fill, fontSize: params.fontSize }),
  highlight: (params) => ({ type: 'rect', x: params.x, y: params.y, w: params.w, h: params.h, fill: params.fill, opacity: params.opacity }),
  mosaic: (params) => ({ type: 'rect', x: params.x, y: params.y, w: params.w, h: params.h, mosaic: true }),
  blur: (params) => ({ type: 'rect', x: params.x, y: params.y, w: params.w, h: params.h, blur: params.blur }),
  crop: (params) => ({ type: 'rect', x: params.x, y: params.y, w: params.w, h: params.h, crop: true }),
  rotate: (params) => ({ type: 'transform', deg: params.deg }),
  flip: (params) => ({ type: 'transform', axis: params.axis }),
};

// 图层 → 渲染指令序列（对齐 Annotation.Draw 把 type+params 转绘制调用的语义）。
export function layerToRenderCommands(layer) {
  if (!layer || typeof layer !== 'object') {
    throw new TypeError('图层对象缺失');
  }
  const factory = RENDER_COMMANDS[layer.type];
  if (!factory) {
    throw new Error(`不支持的渲染类型: ${layer.type}`);
  }
  return [factory(layer.params || {})];
}

// 扁平化单层渲染：编辑完成时 canvas 已把指令绘入位图，返回 flat 层
//（对齐 FlattenAnnotations 后仅剩底层位图）。
export function flattenRenderCommands(layers) {
  if (!Array.isArray(layers)) {
    throw new TypeError('图层列表缺失');
  }
  const commands = layers.flatMap(layerToRenderCommands);
  return { commands, hasCrop: commands.some((cmd) => cmd.crop === true) };
}