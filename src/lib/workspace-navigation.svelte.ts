import { createContext } from 'svelte';

export type MainModuleId = 'sessions' | 'skills' | 'tool-calls';

export type WorkspaceTarget =
  | { module: 'sessions'; sessionId?: string; eventId?: string }
  | { module: 'skills' }
  | { module: 'tool-calls' };

export class WorkspaceNavigation {
  activeModule = $state<MainModuleId>('sessions');
  sessionTarget = $state.raw<{ sessionId: string; eventId?: string } | null>(null);

  navigate(target: WorkspaceTarget) {
    this.activeModule = target.module;
    if (target.module === 'sessions' && target.sessionId) {
      this.sessionTarget = { sessionId: target.sessionId, eventId: target.eventId };
    }
  }

  clearSessionTarget() {
    this.sessionTarget = null;
  }
}

export const [getWorkspaceNavigation, setWorkspaceNavigation] =
  createContext<WorkspaceNavigation>();
