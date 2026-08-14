export interface SessionPageRequest {
  page: number;
  pageSize: number;
  query?: string | null;
  archived?: boolean | null;
}

export interface SessionPage {
  items: SessionListItem[];
  page: number;
  pageSize: number;
  total: number;
  generatedAtMs: number;
}

export interface SessionListItem {
  id: string;
  sourceSessionId: string;
  provider: string;
  title: string;
  projectName: string;
  cwd: string | null;
  createdAtMs: number | null;
  updatedAtMs: number | null;
  tokensUsed: number;
  invocationCount: number;
  skillInvocationCount: number;
  eventCount: number;
  archived: boolean;
  status: string;
  model: string | null;
  modelProvider: string | null;
  agentVersion: string | null;
  gitBranch: string | null;
}

export interface SessionDetail {
  session: SessionListItem;
  agentName: string | null;
  agentRole: string | null;
  gitCommit: string | null;
  gitRemoteUrl: string | null;
  dataQuality: string;
  events: SessionEventItem[];
}

export interface SessionEventItem {
  id: string;
  invocationId: string | null;
  timestampMs: number | null;
  eventType: string;
  event: NormalizedEvent;
}

export interface NormalizedEvent {
  external_id: string | null;
  sequence: {
    position: number;
    part: number;
    logical_ordinal?: number | null;
  };
  parent: string | null;
  inherited_from?: string | null;
  actor: string | Record<string, string>;
  agent_id: string | null;
  data: NormalizedEventData;
}

export interface NormalizedEventData {
  type: string;
  value?: unknown;
}

export interface MessageEventValue {
  role: string;
  phase?: string | Record<string, string> | null;
  content: ContentBlock[];
}

export interface ToolCallEventValue {
  call_id: string;
  name: string;
  namespace?: string | null;
  source_kind?: string;
  server_name?: string | null;
  title?: string | null;
  kind: string;
  status: string;
  input: unknown;
  locations: Array<{ path: string; line?: number | null }>;
}

export interface ToolResultEventValue {
  call_id: string;
  name?: string | null;
  output: unknown;
  content: ContentBlock[];
  status: string;
  error?: string | null;
  duration_ms?: number | null;
}

export interface ReasoningEventValue {
  summary: string[];
  content: ContentBlock[];
  visibility?: string;
}

export type ContentBlock =
  | {
      type: 'text';
      text: string;
    }
  | {
      type: 'image' | 'audio';
      mime_type?: string | null;
      uri?: string | null;
      data?: string | null;
    }
  | {
      type: 'resource';
      uri: string;
      mime_type?: string | null;
      text?: string | null;
      data?: string | null;
    }
  | {
      type: 'resource_link';
      uri: string;
      name?: string | null;
      title?: string | null;
      description?: string | null;
      mime_type?: string | null;
      size?: number | null;
    }
  | {
      type: 'unknown';
      kind?: string | null;
      value: unknown;
    };

export async function getSessionPage(request: SessionPageRequest): Promise<SessionPage> {
  const { invoke, isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) {
    return {
      items: [],
      page: Math.max(request.page, 1),
      pageSize: request.pageSize || 25,
      total: 0,
      generatedAtMs: Date.now(),
    };
  }
  return invoke<SessionPage>('get_session_page', { request });
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail | null> {
  const { invoke, isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) return null;
  return invoke<SessionDetail | null>('get_session_detail', { sessionId });
}
