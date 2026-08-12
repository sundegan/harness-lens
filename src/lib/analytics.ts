export interface SkillAnalysisData {
  skills: SkillSummary[];
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

export async function getSkillAnalysis(): Promise<SkillAnalysisData> {
  const { invoke, isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) {
    return { skills: [] };
  }
  return invoke<SkillAnalysisData>('get_skill_analysis');
}
