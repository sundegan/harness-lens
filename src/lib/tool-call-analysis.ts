import type { SessionEventItem } from '$lib/session-history';

export type TrendBucket = 'hour' | 'day' | 'week' | 'month';

export interface ToolCallFilters {
  startAtMs?: number | null;
  endAtMs?: number | null;
  providers: string[];
  projectKeys: string[];
  toolNames: string[];
  mcpServers: string[];
  timezone: string;
  bucket: TrendBucket;
}

export interface MetricRate {
  numerator: number;
  denominator: number;
  rate: number | null;
  unknownCount: number;
}

export interface ToolCallSummary {
  callCount: number;
  toolCount: number;
  sessionCount: number;
  projectCount: number;
  averageDurationMs: number | null;
  successRate: MetricRate;
}

export interface ToolCallTrendPoint {
  bucketStartMs: number;
  label: string;
  callCount: number;
  failedCount: number;
}

export interface ToolCallComparison {
  key: string;
  label: string;
  callCount: number;
  sessionCount: number;
  projectCount: number;
  failedCount: number;
  declinedCount: number;
  cancelledCount: number;
  averageDurationMs: number | null;
  exactRepeatCount: number;
  successRate: number | null;
}

export interface ToolCallAnalysisData {
  summary: ToolCallSummary;
  timeTrend: ToolCallTrendPoint[];
  statusDistribution: ToolCallComparison[];
  toolRanking: ToolCallComparison[];
  providerComparison: ToolCallComparison[];
  projectComparison: ToolCallComparison[];
  mcpServerComparison: ToolCallComparison[];
  generatedAtMs: number;
}

export interface ToolCallFilterOption {
  value: string;
  label: string;
  count: number;
}

export interface ToolCallFilterOptions {
  providers: ToolCallFilterOption[];
  projects: ToolCallFilterOption[];
  toolNames: ToolCallFilterOption[];
  mcpServers: ToolCallFilterOption[];
}

export interface ToolCallListItem {
  id: string;
  provider: string;
  sessionId: string;
  sourceSessionId: string;
  sessionTitle: string;
  projectName: string;
  agentVersion: string | null;
  toolName: string;
  mcpServer: string | null;
  toolKind: string;
  startedAtMs: number | null;
  completedAtMs: number | null;
  durationMs: number | null;
  status: string;
  hasResult: boolean;
  callEventId: string | null;
  resultEventId: string | null;
}

export interface ToolCallPageRequest extends ToolCallFilters {
  page: number;
  pageSize: number;
  query?: string | null;
  sortBy?: string | null;
  sortDirection?: 'asc' | 'desc' | null;
}

export interface ToolCallPage {
  items: ToolCallListItem[];
  page: number;
  pageSize: number;
  total: number;
  generatedAtMs: number;
}

export interface ToolCallDetail {
  call: ToolCallListItem;
  callEvent: SessionEventItem | null;
  resultEvent: SessionEventItem | null;
  sessionEventId: string | null;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

export function defaultToolCallFilters(): ToolCallFilters {
  return {
    startAtMs: Date.now() - 30 * 24 * 60 * 60 * 1000,
    endAtMs: null,
    providers: [],
    projectKeys: [],
    toolNames: [],
    mcpServers: [],
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
    bucket: 'day',
  };
}

export function getToolCallAnalysis(filters: ToolCallFilters): Promise<ToolCallAnalysisData> {
  return invoke('get_tool_call_analysis', { request: filters });
}

export function getToolCallFilterOptions(filters: ToolCallFilters): Promise<ToolCallFilterOptions> {
  return invoke('get_tool_call_filter_options', { request: filters });
}

export function getToolCallPage(request: ToolCallPageRequest): Promise<ToolCallPage> {
  return invoke('get_tool_call_page', { request });
}

export function getToolCallDetail(toolCallId: string): Promise<ToolCallDetail | null> {
  return invoke('get_tool_call_detail', { toolCallId });
}
