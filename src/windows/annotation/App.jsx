// R3 图像编辑器前端：画布标注（对齐 ShareX.ImageEditor 语义）。
// 把三个纯函数 model（layerModel/renderModel/historyModel）接成可用
// 编辑器：加载截图 → 工具面板选型 → 画布拖绘成标注层 → 撤销/重做 →
// 保存把画布栅格化（含全部标注层）转 PNG base64 走 save_img_png_base64
// 落剪贴板历史（复用截图存储链路）后关窗；取消直接关窗不保存。

import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { showError } from '@shared/utils/dialog';
import SimpleInputDialog from './SimpleInputDialog';
import {
  createLayer,
  addLayer,
  updateLayer,
  removeLayer,
  clearLayers as newEmptyLayers,
  reorderLayer,
} from './layerModel';
import { layerToRenderCommands, flattenRenderCommands } from './renderModel';
import {
  pushSnapshot,
  undoSnapshot,
  redoSnapshot,
  canUndoSnapshots,
  canRedoSnapshots,
  advanceSnapshotGeneration,
} from './historyModel';

const ANNOTATION_LOAD_EVENT = 'annotation:load';
// 工具栏只暴露已实现的绘制工具。mosaic/blur(像素化/模糊)标注尚未实现
// (拖动只会降级成空心矩形,制造"已打码"假象),待专项实现后再启用。
const TOOL_TYPES = ['pen', 'rect', 'ellipse', 'line', 'arrow', 'text', 'highlight'];
const STROKE_COLORS = ['#ef4444', '#3b82f6', '#22c55e', '#eab308', '#1f2937', '#ffffff'];

// 由 RENDER_COMMANDS 补全的变换类指令（crop/rotate/flip 由保存路径做
// 位图变换，此处仅透传标记由后端/pixel 操作承接）。
const TRANSFORM_TYPES = ['crop', 'rotate', 'flip'];

function pointFromEvent(event, rect) {
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

function drawLayersToContext(ctx, layers, imageWidth, imageHeight) {
  // 标注层 → 渲染指令：基础位图之后逐层执行 2D 绘制（对齐 Annotation.Draw）。
  // 变换层（rotate/flip/crop）按出现顺序作用于整个画布，绘制后的位图在
  // 保存路径做最终变换。
  for (const layer of layers) {
    if (TRANSFORM_TYPES.includes(layer.type)) {
      continue;
    }
    const [command] = layerToRenderCommands(layer);
    ctx.save();
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    switch (command.type) {
      case 'rect':
      case 'ellipse': {
        ctx.strokeStyle = command.stroke || '#ef4444';
        ctx.lineWidth = command.width || 2;
        ctx.beginPath();
        if (command.type === 'rect') {
          ctx.rect(command.x, command.y, command.w, command.h);
        } else {
          ctx.ellipse(command.x + command.w / 2, command.y + command.h / 2, Math.abs(command.w) / 2, Math.abs(command.h) / 2, 0, 0, Math.PI * 2);
        }
        if (command.fill) {
          ctx.fillStyle = command.fill;
          ctx.globalAlpha = command.opacity ?? 1;
          ctx.fill();
          ctx.globalAlpha = 1;
        }
        ctx.stroke();
        break;
      }
      case 'line':
      case 'arrow': {
        ctx.strokeStyle = command.stroke || '#ef4444';
        ctx.lineWidth = command.width || 2;
        ctx.beginPath();
        ctx.moveTo(command.x1, command.y1);
        ctx.lineTo(command.x2, command.y2);
        ctx.stroke();
        if (command.type === 'arrow') {
          const angle = Math.atan2(command.y2 - command.y1, command.x2 - command.x1);
          const head = 12;
          ctx.beginPath();
          ctx.moveTo(command.x2, command.y2);
          ctx.lineTo(command.x2 - head * Math.cos(angle - 0.4), command.y2 - head * Math.sin(angle - 0.4));
          ctx.moveTo(command.x2, command.y2);
          ctx.lineTo(command.x2 - head * Math.cos(angle + 0.4), command.y2 - head * Math.sin(angle + 0.4));
          ctx.stroke();
        }
        break;
      }
      case 'polyline': {
        ctx.strokeStyle = command.stroke || '#ef4444';
        ctx.lineWidth = command.width || 2;
        ctx.beginPath();
        if (command.points && command.points.length > 0) {
          ctx.moveTo(command.points[0].x, command.points[0].y);
          for (let i = 1; i < command.points.length; i += 1) {
            ctx.lineTo(command.points[i].x, command.points[i].y);
          }
          ctx.stroke();
        }
        break;
      }
      case 'text': {
        ctx.fillStyle = command.fill || '#ef4444';
        ctx.font = `${command.fontSize || 24}px sans-serif`;
        ctx.fillText(command.text || '', command.x, command.y);
        break;
      }
      default:
        break;
    }
    ctx.restore();
  }
}

function AnnotationApp() {
  const { t } = useTranslation();
  const containerRef = useRef(null);
  const canvasRef = useRef(null);
  const imageRef = useRef(null);
  const [tool, setTool] = useState('pen');
  const [stroke, setStroke] = useState('#ef4444');
  const [layers, setLayers] = useState([]);
  const [history, setHistory] = useState([]);
  const [redoStack, setRedoStack] = useState([]);
  const generationRef = useRef(0);
  const draftRef = useRef(null);
  // 显示空间映射：draw() 每次重绘记录当前缩放与居中偏移，保存时用同一
  // 映射把显示坐标逆算回原图像素（缩放显示时标注不错位）。
  const displayTransformRef = useRef({ scale: 1, offsetX: 0, offsetY: 0 });
  const [imagePath, setImagePath] = useState('');
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });
  // 文本标注输入浮层状态:非空时渲染 SimpleInputDialog,确认后提交文本
  // 图层,取消/Esc 则丢弃——替代 window.prompt(WebView2 可能返回 null)。
  const [textDraft, setTextDraft] = useState(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const image = imageRef.current;
    if (!canvas || !image) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    const displayWidth = canvas.clientWidth;
    const displayHeight = canvas.clientHeight;
    canvas.width = displayWidth * dpr;
    canvas.height = displayHeight * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // 底色白（保存路径也合成白底）避免透明画布与网格错位。
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, displayWidth, displayHeight);
    // 图像等比缩放入画布并记录画布坐标映射（保存时按原尺寸 1:1 导出）。
    const scale = Math.min(displayWidth / image.naturalWidth, displayHeight / image.naturalHeight, 1);
    const offsetX = (displayWidth - image.naturalWidth * scale) / 2;
    const offsetY = (displayHeight - image.naturalHeight * scale) / 2;
    displayTransformRef.current = { scale, offsetX, offsetY };
    ctx.drawImage(image, offsetX, offsetY, image.naturalWidth * scale, image.naturalHeight * scale);
    ctx.save();
    // 标注层绘制于画布坐标（叠加在图像上方）。
    drawLayersToContext(ctx, layers, image.naturalWidth, image.naturalHeight);
    // 正在拖绘的草稿。
    if (draftRef.current) {
      const { type, start, end, points } = draftRef.current;
      const layer = createLayer('draft', type, draftParamsFor(type, start, end), points);
      layer.type = type;
      const [cmd] = reinterpretDraft(layer);
      if (cmd) {
        drawLayersToContext(ctx, [{ id: 'draft', type, params: cmd.params, points }], image.naturalWidth, image.naturalHeight);
      }
    }
    ctx.restore();
  }, [layers]);

  function draftParamsFor(type, start, end) {
    if (type === 'pen') return { stroke, width: 3 };
    if (type === 'text') return { text: '', x: start.x, y: start.y, fill: stroke, fontSize: 24 };
    if (type === 'highlight') return { x: start.x, y: start.y, w: end.x - start.x, h: end.y - start.y, fill: '#fef08a', opacity: 0.4 };
    if (type === 'line') return { x1: start.x, y1: start.y, x2: end.x, y2: end.y, stroke, width: 3 };
    if (type === 'arrow') return { x1: start.x, y1: start.y, x2: end.x, y2: end.y, stroke, width: 3 };
    return { x: start.x, y: start.y, w: end.x - start.x, h: end.y - start.y, stroke, fill: type === 'rect' ? '' : '', width: 2 };
  }

  function reinterpretDraft(layer) {
    const [command] = layerToRenderCommands({ ...layer, params: layer.params });
    // renderModel 对 rect/ellipse 用 stroke/fill；草稿统一转对应指令对象。
    return { params: command, ok: true };
  }

  useEffect(() => {
    draw();
  }, [draw, imageSize]);

  // 窗口尺寸/跨 DPI 显示器拖动后画布物理像素尺寸不变,位图被 CSS 拉伸
  // 显示模糊:监听窗口 resize 与 devicePixelRatio 变化重跑 draw 重建位图。
  useEffect(() => {
    const win = getCurrentWindow();
    const redraw = () => {
      if (!imageRef.current) return;
      draw();
    };
    let unlistenResized;
    win
      .onResized(redraw)
      .then((u) => (unlistenResized = u))
      .catch(() => {});
    // 跨 DPI 显示器移动不触发窗口尺寸事件,用 matchMedia 订阅 dppx 变化
    // (分辨率档位变化时 change 事件触发),重建查询以拿到新的 dppx。
    const mq = window.matchMedia('(resolution: ' + window.devicePixelRatio + 'dppx)');
    const onDpiChange = () => {
      redraw();
    };
    mq.addEventListener('change', onDpiChange);
    return () => {
      unlistenResized?.();
      mq.removeEventListener('change', onDpiChange);
    };
  }, [draw]);

  useEffect(() => {
    let unlistenPromise;
    // 监听截图动作链推送的待编辑图片路径。
    unlistenPromise = listen(ANNOTATION_LOAD_EVENT, (event) => {
      const path = String(event.payload || '');
      setImagePath(path);
      const img = new Image();
      img.onload = () => {
        imageRef.current = img;
        setImageSize({ width: img.naturalWidth, height: img.naturalHeight });
        // 加载新图前先丢弃进行中草稿,否则旧草稿残留会被新图画布 draw 渲染
        // (坐标错位)+首指守卫吞掉新图第一笔(与撤销/清空/删除同款兜底)。
        draftRef.current = null;
        // 加载新图时清空历史与标注，避免跨图累积。
        setLayers(newEmptyLayers());
        setHistory([]);
        setRedoStack([]);
        generationRef.current += 1;
      };
      img.onerror = () => {
        void getCurrentWindow().close();
      };
      // 截图存储路径走 asset 协议前缀（与贴图/预览同款）。
      img.src = convertFileSrc(path);
    }).then(() => {
      // 监听就绪后通知后端:首开时窗口新建早于页面加载,后端把待编辑路径
      // 缓存在 pending,此调用触发重放,避免事件在页面就绪前被丢弃(白屏)。
      void invoke('annotation_window_ready').catch(() => {});
    });
    return () => {
      unlistenPromise?.then((unlisten) => unlisten()).catch(() => {});
    };
  }, []);

  const commitLayer = (layer) => {
    // 计算后置位（closure 拿最新 layers），避免在 setState updater 内做
    // 副作用（React StrictMode 会双调用 updater，产生重复历史快照）。
    const next = addLayer(layers, layer, generationRef.current, generationRef);
    setHistory((h) => pushSnapshot(h, layers).history);
    setRedoStack([]);
    setLayers(next);
  };

  const handlePointerDown = (event) => {
    event.preventDefault();
    if (!imageRef.current) return;
    // 多指针守卫:第二根手指(或手掌误触)落下不得覆盖首笔草稿,只认
    // 首指 pointerId(与白板 activePointerId 同款保护)。
    if (draftRef.current && event.pointerId !== draftRef.current.pointerId) return;
    // 捕获指针:绘制中把笔划拖出画布边界时仍能收到 pointerup 收口,
    // 否则指针滑到工具栏上方释放时画布收不到抬起事件,提交半截笔画
    // (与白板同款保护)。
    try {
      canvasRef.current.setPointerCapture(event.pointerId);
    } catch {
      // 个别指针 id 已失效时忽略,后续 move/up 守卫兜底。
    }
    const rect = canvasRef.current.getBoundingClientRect();
    const start = pointFromEvent(event, rect);
    if (tool === 'pen') {
      draftRef.current = { pointerId: event.pointerId, type: tool, start, end: start, points: [start] };
    } else {
      draftRef.current = { pointerId: event.pointerId, type: tool, start, end: start, points: [] };
    }
  };

  const handlePointerMove = (event) => {
    const draft = draftRef.current;
    if (!draft) return;
    // 只处理本笔指针的移动,第二指移动点不得混入当前笔。
    if (event.pointerId !== draft.pointerId) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const point = pointFromEvent(event, rect);
    if (draft.type === 'pen') {
      draftRef.current = { ...draft, end: point, points: [...draft.points, point] };
    } else {
      draftRef.current = { ...draft, end: point };
    }
    draw();
  };

  const handlePointerUp = (event) => {
    const draft = draftRef.current;
    if (!draft) return;
    // 只收口本笔指针,第二指抬起(含 pointerleave 兜底)不得提交混入笔。
    if (event?.pointerId !== undefined && event.pointerId !== draft.pointerId) return;
    draftRef.current = null;
    // 文本工具：打开自绘输入浮层取文本(替代 window.prompt——WebView2
    // 对 prompt 支持有别于常规浏览器,可能返回 null 让文本标注静默失效)。
    if (draft.type === 'text') {
      setTextDraft({ x: draft.start.x, y: draft.start.y });
      draw();
      return;
    }
    const params = draftParamsFor(draft.type, draft.start, draft.end);
    // 钢笔的自由路径点要放进 params.points（renderModel 的 pen 指令读
    // params.points 而非图层 points 字段）。
    if (draft.type === 'pen') {
      params.points = draft.points;
    }
    const layer = createLayer(String(Date.now()), draft.type, params, draft.type === 'pen' ? draft.points : [draft.start, draft.end]);
    commitLayer(layer);
    draw();
  };

  const handleUndo = () => {
    // 先丢弃进行中草稿(不入栈),避免撤销后残留半截笔画悬浮不可撤销。
    draftRef.current = null;
    const result = undoSnapshot(history, redoStack);
    if (!result) return;
    setHistory(result.history);
    setRedoStack(result.redoStack);
    setLayers(result.snapshot === null ? [] : result.snapshot);
  };

  const handleRedo = () => {
    // 同 undo:先清草稿再弹栈,保证画布与历史栈一致。
    draftRef.current = null;
    const result = redoSnapshot(history, redoStack);
    if (!result) return;
    setHistory(result.history);
    setRedoStack(result.redoStack);
    setLayers(result.snapshot || []);
  };

  const handleClear = () => {
    // 同 undo/redo:先丢弃进行中草稿,避免清空后 draw 重跑渲染悬浮旧草稿
    // (不可撤销不可保存,松手还会补成正式图层诈尸)。
    draftRef.current = null;
    setHistory((h) => pushSnapshot(h, layers).history);
    setRedoStack([]);
    setLayers(newEmptyLayers());
  };

  const handleRemoveSelected = (id) => {
    // 同 handleClear:删除图层前先清进行中草稿,防悬浮残留。
    draftRef.current = null;
    setHistory((h) => pushSnapshot(h, layers).history);
    setRedoStack([]);
    setLayers((prev) => removeLayer(prev, id));
  };

  const handleSave = async () => {
    // 保存：以图像原始尺寸重绘（等比放大到原图），含全部标注层 → PNG。
    const image = imageRef.current;
    if (!image) return;
    const out = document.createElement('canvas');
    // 导出画布必须与原图像素尺寸一致——不能乘 dpr,否则高分屏上产物被
    // 放大 dpr 倍(1920×1080@2x → 3840×2160,体积 2-4 倍+重采样损质)。
    // 显示 DPI 只影响屏幕显示,不应烤进输出位图。
    out.width = image.naturalWidth;
    out.height = image.naturalHeight;
    const octx = out.getContext('2d');
    octx.fillStyle = '#ffffff';
    octx.fillRect(0, 0, image.naturalWidth, image.naturalHeight);
    octx.drawImage(image, 0, 0, image.naturalWidth, image.naturalHeight);
    // 标注坐标记录于显示空间（CSS 像素），缩放显示时须先把当前映射逆算回
    // 原图像素：图像坐标 = (显示坐标 - 居中偏移) / scale，设备像素再乘 dpr，
    // 故平移项符号为负（加号会让标注向右下漂移 offset/scale 像素）。
    const { scale, offsetX, offsetY } = displayTransformRef.current;
    octx.setTransform(1 / scale, 0, 0, 1 / scale, -offsetX / scale, -offsetY / scale);
    drawLayersToContext(octx, layers, image.naturalWidth, image.naturalHeight);
    const base64 = out.toDataURL('image/png').split(',')[1];
    try {
      // 落剪贴板历史复用通用画布保存命令（与白板一致）。
      await invoke('save_img_png_base64', { pngBase64: base64 });
    } catch (error) {
      // 保存失败(落盘/剪贴板/历史写库任一步)必须提示用户并保持窗口打开——
      // 无条件关窗会让用户误以为保存成功,标注成果静默丢失。
      console.error('保存编辑器结果失败:', error);
      await showError('保存失败,请重试:' + String(error?.message || error));
      return;
    }
    await getCurrentWindow().close().catch(() => {});
  };

  const handleCancel = async () => {
    await getCurrentWindow().close().catch(() => {});
  };

  return (
    <div className="flex h-full flex-col bg-qc-panel text-qc-fg" style={{ height: '100%' }}>
      <div className="flex items-center gap-2 border-b border-qc-border px-3 py-1.5">
        <div className="flex flex-wrap items-center gap-1">
          {TOOL_TYPES.map((type) => (
            <button key={type} type="button" onClick={() => setTool(type)} className={`rounded-md px-2 py-1 text-xs ${tool === type ? 'bg-qc-primary text-qc-panel' : 'hover:bg-qc-hover'}`}>
              {type}
            </button>
          ))}
        </div>
        <div className="ml-2 flex items-center gap-1">
          {STROKE_COLORS.map((color) => (
            <button key={color} type="button" onClick={() => setStroke(color)} className={`h-4 w-4 rounded-full border ${stroke === color ? 'ring-2 ring-qc-primary' : 'border-qc-border'}`} style={{ background: color }} />
          ))}
        </div>
        <div className="ml-auto flex items-center gap-1">
          <button type="button" disabled={!canUndoSnapshots(history)} onClick={handleUndo} className="rounded-md px-2 py-1 text-xs hover:bg-qc-hover disabled:opacity-40">{t('annotation.undo', { defaultValue: '撤销' })}</button>
          <button type="button" disabled={!canRedoSnapshots(redoStack)} onClick={handleRedo} className="rounded-md px-2 py-1 text-xs hover:bg-qc-hover disabled:opacity-40">{t('annotation.redo', { defaultValue: '重做' })}</button>
          <button type="button" onClick={handleClear} className="rounded-md px-2 py-1 text-xs hover:bg-qc-hover">{t('annotation.clear', { defaultValue: '清空' })}</button>
          <button type="button" onClick={handleCancel} className="rounded-md px-2 py-1 text-xs hover:bg-qc-hover">{t('annotation.cancel', { defaultValue: '取消' })}</button>
          <button type="button" onClick={handleSave} className="rounded-md bg-qc-primary px-2.5 py-1 text-xs text-qc-panel">{t('annotation.doneAndCopy', { defaultValue: '完成并复制' })}</button>
        </div>
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden bg-qc-panel-2">
        <canvas ref={canvasRef} className="h-full w-full touch-none" onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} onPointerLeave={handlePointerUp} onPointerCancel={handlePointerUp} />
        {textDraft && (
          <SimpleInputDialog
            title={t('annotation.textPrompt', { defaultValue: '输入标注文本' })}
            value=""
            onChange={() => {}}
            onConfirm={(label) => {
              const trimmed = (label || '').trim();
              if (trimmed) {
                const layer = createLayer(String(Date.now()), 'text', { ...draftParamsFor('text', textDraft, textDraft), text: trimmed }, [textDraft]);
                commitLayer(layer);
              }
              setTextDraft(null);
              draw();
            }}
            onCancel={() => {
              setTextDraft(null);
              draw();
            }}
            placeholder=""
            confirmText={t('annotation.confirmText', { defaultValue: '确认' })}
            cancelText={t('annotation.cancel', { defaultValue: '取消' })}
          />
        )}
      </div>
      <div className="pointer-events-none absolute right-2 top-1 text-xs text-qc-fg-muted">
        {imageSize.width > 0 ? `${imageSize.width} × ${imageSize.height}` : t('annotation.waitingLoad', { defaultValue: '等待加载截图…' })}
      </div>
      {layers.length > 0 && (
        <div className="border-t border-qc-border px-3 py-1">
          <div className="flex max-h-12 flex-wrap gap-1 overflow-auto text-[11px] text-qc-fg-muted">
            {layers.map((layer) => (
              <span key={layer.id} className="flex items-center gap-1 rounded bg-qc-hover px-1.5 py-0.5">
                {layer.type}
                <button type="button" className="text-qc-danger" onClick={() => handleRemoveSelected(layer.id)}>×</button>
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(<AnnotationApp />);
}

export default AnnotationApp;