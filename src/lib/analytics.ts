export type AnalyticsSyncState = 'not_started' | 'syncing' | 'ready' | 'error' | 'unavailable';

export type AnalyticsSyncPhase = 'idle' | 'discovery' | 'initial_scan' | 'watching';

export type AnalyticsInvocationStatus =
  | 'in_progress'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'unknown';

export interface AnalyticsSnapshot {
  sync: AnalyticsSyncStatus;
  summary: OverallSummary;
  sessions: SessionSummary[];
  skills: SkillSummary[];
  generatedAtMs: number;
}

export interface AnalyticsSyncStatus {
  status: AnalyticsSyncState;
  phase: AnalyticsSyncPhase;
  processedRecords: number;
  diagnosticCount: number;
  lastError: string | null;
  updatedAtMs: number | null;
}

export interface OverallSummary {
  sessionCount: number;
  totalTokens: number;
  skillInvocationCount: number;
  succeededInvocationCount: number;
  failedInvocationCount: number;
  cancelledInvocationCount: number;
  activeInvocationCount: number;
  unknownInvocationCount: number;
}

export interface SessionSummary {
  id: string;
  title: string;
  projectName: string;
  createdAtMs: number | null;
  updatedAtMs: number | null;
  observedDurationMs: number;
  wallDurationMs: number | null;
  tokensUsed: number;
  invocationCount: number;
  skillInvocationCount: number;
  archived: boolean;
  status: AnalyticsInvocationStatus;
}

export interface SkillSummary {
  name: string;
  invocationCount: number;
  succeededCount: number;
  failedCount: number;
  cancelledCount: number;
  unknownCount: number;
  successRate: number | null;
  averageDurationMs: number | null;
  maxDurationMs: number | null;
  averageTokens: number | null;
  maxTokens: number | null;
}

export async function getAnalyticsSnapshot(): Promise<AnalyticsSnapshot> {
  const { invoke, isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) {
    return emptySnapshot();
  }
  return invoke<AnalyticsSnapshot>('get_analytics_snapshot');
}

function emptySnapshot(): AnalyticsSnapshot {
  return {
    sync: {
      status: 'not_started',
      phase: 'idle',
      processedRecords: 0,
      diagnosticCount: 0,
      lastError: null,
      updatedAtMs: null,
    },
    summary: {
      sessionCount: 0,
      totalTokens: 0,
      skillInvocationCount: 0,
      succeededInvocationCount: 0,
      failedInvocationCount: 0,
      cancelledInvocationCount: 0,
      activeInvocationCount: 0,
      unknownInvocationCount: 0,
    },
    sessions: [],
    skills: [],
    generatedAtMs: Date.now(),
  };
}
