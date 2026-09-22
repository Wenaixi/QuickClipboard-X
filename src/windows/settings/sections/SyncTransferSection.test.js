import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./SyncTransferSection.jsx', import.meta.url), 'utf8');

test('自动同步报告必须渲染失败原因(errors 列表)', () => {
  // 自动推送/拉取失败在后端经 store_report 写入 LanAutoSyncStatus.last_report,
  // 若失败报告计数全 0 只显示"成功"会把失败静默吞掉——LanAutoReportCard
  // 必须对 result.errors 非空渲染错误列表(用户可见失败原因)。
  assert.ok(
    source.includes('Array.isArray(report.result?.errors)'),
    'LanAutoReportCard 必须提取 report.result.errors 数组',
  );
  assert.ok(
    source.includes('errors.map((error, index) =>'),
    'errors 非空必须逐条渲染错误列表',
  );
});

test('自动同步状态消费 last_report 并渲染(零渲染缺陷护栏)', () => {
  // R124 诊断:autoSyncStatus 此前只被 LanModePanel 消费 .settings,
  // last_report 在后端已存储并经 listen 触发刷新,但前端零渲染——自动
  // 推送失败(设备离线/配对移除)用户完全不可见。修复要求新增
  // LanAutoReportCard 并渲染 autoSyncStatus?.last_report。
  assert.ok(
    source.includes('function LanAutoReportCard'),
    '必须存在 LanAutoReportCard 组件渲染自动同步报告',
  );
  assert.ok(
    source.includes('autoSyncStatus?.last_report'),
    'LanAutoReportCard 必须消费 autoSyncStatus.last_report',
  );
  assert.ok(
    source.includes("listen('sync-transfer-lan-report'"),
    '必须监听 sync-transfer-lan-report 使失败实时可见',
  );
});
