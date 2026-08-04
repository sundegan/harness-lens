<script lang="ts">
import { onMount } from 'svelte';
import {
  type AnalyticsSnapshot,
  type AnalyticsSyncState,
  type AnalyticsTurnStatus,
  getAnalyticsSnapshot,
  type SessionSummary,
  type SkillSummary,
} from '$lib/analytics';
import { i18nManager } from '$lib/i18n.svelte';
import { logError } from '$lib/logger';
import AboutDialog from '$lib/menu/AboutDialog.svelte';

type DashboardView = 'sessions' | 'skills';
type SessionFilter = 'all' | 'current' | 'archived';
const DASHBOARD_PAGE_SIZE = 50;

let snapshot = $state.raw<AnalyticsSnapshot | null>(null);
let activeView = $state<DashboardView>('sessions');
let sessionFilter = $state<SessionFilter>('all');
let query = $state('');
let sessionPage = $state(1);
let skillPage = $state(1);
let isLoading = $state(true);
let loadError = $state('');
let requestSequence = 0;

const normalizedQuery = $derived(query.trim().toLocaleLowerCase());
const visibleSessions = $derived.by(() => {
  if (!snapshot) return [];
  return snapshot.sessions.filter((session) => {
    const matchesArchive =
      sessionFilter === 'all' ||
      (sessionFilter === 'archived' ? session.archived : !session.archived);
    if (!matchesArchive) return false;
    if (!normalizedQuery) return true;
    return (
      session.title.toLocaleLowerCase().includes(normalizedQuery) ||
      session.projectName.toLocaleLowerCase().includes(normalizedQuery)
    );
  });
});
const visibleSkills = $derived.by(() => {
  if (!snapshot) return [];
  if (!normalizedQuery) return snapshot.skills;
  return snapshot.skills.filter((skill) =>
    skill.name.toLocaleLowerCase().includes(normalizedQuery)
  );
});
const sessionPageCount = $derived(
  Math.max(1, Math.ceil(visibleSessions.length / DASHBOARD_PAGE_SIZE))
);
const skillPageCount = $derived(Math.max(1, Math.ceil(visibleSkills.length / DASHBOARD_PAGE_SIZE)));
const activeSessionPage = $derived(Math.min(sessionPage, sessionPageCount));
const activeSkillPage = $derived(Math.min(skillPage, skillPageCount));
const activePage = $derived(activeView === 'sessions' ? activeSessionPage : activeSkillPage);
const activePageCount = $derived(activeView === 'sessions' ? sessionPageCount : skillPageCount);
const pagedSessions = $derived.by(() => {
  const start = (activeSessionPage - 1) * DASHBOARD_PAGE_SIZE;
  return visibleSessions.slice(start, start + DASHBOARD_PAGE_SIZE);
});
const pagedSkills = $derived.by(() => {
  const start = (activeSkillPage - 1) * DASHBOARD_PAGE_SIZE;
  return visibleSkills.slice(start, start + DASHBOARD_PAGE_SIZE);
});
const dashboardLocale = $derived.by(() => {
  const language = i18nManager.resolvedLanguage;
  if (language === 'zh') return 'zh-CN';
  if (language === 'zh_tw') return 'zh-TW';
  return language;
});
const integerFormatter = $derived(
  new Intl.NumberFormat(dashboardLocale, { maximumFractionDigits: 0 })
);
const compactIntegerFormatter = $derived(
  new Intl.NumberFormat(dashboardLocale, {
    notation: 'compact',
    maximumFractionDigits: 1,
  })
);
const decimalFormatter = $derived(
  new Intl.NumberFormat(dashboardLocale, { maximumFractionDigits: 1 })
);
const dateFormatter = $derived(
  new Intl.DateTimeFormat(dashboardLocale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
);
const totalTurnCount = $derived.by(() => {
  if (!snapshot) return 0;
  const summary = snapshot.summary;
  return (
    summary.succeededTurnCount +
    summary.failedTurnCount +
    summary.cancelledTurnCount +
    summary.activeTurnCount +
    summary.unknownTurnCount
  );
});
const outcomeSegments = $derived.by(() => {
  if (!snapshot) return [];
  return [
    {
      key: 'succeeded',
      value: snapshot.summary.succeededTurnCount,
      label: i18nManager.t('dashboard.turns.succeeded', {
        count: integerFormatter.format(snapshot.summary.succeededTurnCount),
      }),
    },
    {
      key: 'failed',
      value: snapshot.summary.failedTurnCount,
      label: i18nManager.t('dashboard.turns.failed', {
        count: integerFormatter.format(snapshot.summary.failedTurnCount),
      }),
    },
    {
      key: 'cancelled',
      value: snapshot.summary.cancelledTurnCount,
      label: i18nManager.t('dashboard.turns.cancelled', {
        count: integerFormatter.format(snapshot.summary.cancelledTurnCount),
      }),
    },
    {
      key: 'active',
      value: snapshot.summary.activeTurnCount,
      label: i18nManager.t('dashboard.turns.active', {
        count: integerFormatter.format(snapshot.summary.activeTurnCount),
      }),
    },
    {
      key: 'unknown',
      value: snapshot.summary.unknownTurnCount,
      label: i18nManager.t('dashboard.turns.unknown', {
        count: integerFormatter.format(snapshot.summary.unknownTurnCount),
      }),
    },
  ];
});
const outcomeDescription = $derived(outcomeSegments.map((segment) => segment.label).join(', '));

const loadSnapshot = async () => {
  const sequence = ++requestSequence;
  isLoading = snapshot === null;
  try {
    const next = await getAnalyticsSnapshot();
    if (sequence !== requestSequence) return;
    snapshot = next;
    loadError = '';
  } catch (error) {
    if (sequence !== requestSequence) return;
    loadError = error instanceof Error ? error.message : String(error);
    logError('Failed to load analytics', error);
  } finally {
    if (sequence === requestSequence) {
      isLoading = false;
    }
  }
};

onMount(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;

  void loadSnapshot();
  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { listen } = await import('@tauri-apps/api/event');
    const stopListening = await listen('analytics-updated', () => {
      if (!disposed) void loadSnapshot();
    });
    if (disposed) {
      stopListening();
    } else {
      unlisten = stopListening;
    }
  })().catch((error) => logError('Failed to listen for analytics updates', error));

  return () => {
    disposed = true;
    unlisten?.();
  };
});

const setView = (view: DashboardView) => {
  activeView = view;
  query = '';
  sessionPage = 1;
  skillPage = 1;
};

const setSessionFilter = (filter: SessionFilter) => {
  sessionFilter = filter;
  sessionPage = 1;
};

const handleQueryInput = (event: Event) => {
  const input = event.currentTarget;
  if (!(input instanceof HTMLInputElement)) return;
  query = input.value;
  sessionPage = 1;
  skillPage = 1;
};

const goToPage = (view: DashboardView, page: number) => {
  if (view === 'sessions') {
    sessionPage = Math.min(Math.max(page, 1), sessionPageCount);
  } else {
    skillPage = Math.min(Math.max(page, 1), skillPageCount);
  }
};

const handleViewKeydown = (event: KeyboardEvent) => {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  const nextView = event.key === 'ArrowRight' || event.key === 'End' ? 'skills' : 'sessions';
  setView(nextView);
  requestAnimationFrame(() => {
    document.getElementById(`analytics-${nextView}-tab`)?.focus();
  });
};

const formatInteger = (value: number) => integerFormatter.format(value);

const formatCompactInteger = (value: number) =>
  value >= 10_000 ? compactIntegerFormatter.format(value) : integerFormatter.format(value);

const outcomeWidth = (value: number) =>
  `${totalTurnCount === 0 ? 0 : (value / totalTurnCount) * 100}%`;

const formatDuration = (milliseconds: number | null) => {
  if (milliseconds === null) return '—';
  const seconds = Math.max(milliseconds, 0) / 1000;
  if (seconds < 60) {
    return i18nManager.t('dashboard.duration.seconds', {
      value: decimalFormatter.format(seconds),
    });
  }
  const minutes = seconds / 60;
  if (minutes < 60) {
    return i18nManager.t('dashboard.duration.minutes', {
      value: decimalFormatter.format(minutes),
    });
  }
  const hours = minutes / 60;
  if (hours < 24) {
    return i18nManager.t('dashboard.duration.hours', {
      value: decimalFormatter.format(hours),
    });
  }
  return i18nManager.t('dashboard.duration.days', {
    value: decimalFormatter.format(hours / 24),
  });
};

const formatDate = (timestamp: number | null) => {
  if (timestamp === null) return '—';
  return dateFormatter.format(timestamp);
};

const statusLabel = (status: AnalyticsTurnStatus) =>
  i18nManager.t(`dashboard.status.${status === 'in_progress' ? 'active' : status}`);

const syncLabel = (status: AnalyticsSyncState) => i18nManager.t(`dashboard.sync.${status}`);

const sessionName = (session: SessionSummary) =>
  session.title || session.projectName || i18nManager.t('dashboard.session.untitled');

const outcomeLabel = (skill: SkillSummary) =>
  i18nManager.t('dashboard.skill.outcome_values', {
    succeeded: formatInteger(skill.succeededCount),
    failed: formatInteger(skill.failedCount),
    cancelled: formatInteger(skill.cancelledCount),
    unknown: formatInteger(skill.unknownCount),
  });
</script>

<svelte:head>
  <title>{i18nManager.t('dashboard.title')} | HarnessLens</title>
</svelte:head>

<main class="dashboard" data-testid="analytics-dashboard">
  <header class="dashboard-header">
    <div class="heading-copy">
      <div class="context-line">
        <span class="source-mark" aria-hidden="true">
          <svg viewBox="0 0 20 20">
            <path d="M4 5.5h12M4 10h8M4 14.5h10" />
          </svg>
        </span>
        <p class="eyebrow">{i18nManager.t('dashboard.eyebrow')}</p>
      </div>
      <h1>{i18nManager.t('dashboard.title')}</h1>
      <p class="subtitle">{i18nManager.t('dashboard.subtitle')}</p>
    </div>
    <div class="sync-area" aria-live="polite">
      {#if snapshot}
        <span
          class={[
            'sync-state',
            {
              syncing: snapshot.sync.status === 'syncing',
              error:
                snapshot.sync.status === 'error' || snapshot.sync.status === 'unavailable',
              idle: snapshot.sync.status === 'not_started'
            }
          ]}
        >
          <span class="sync-dot"></span>
          {syncLabel(snapshot.sync.status)}
        </span>
      {/if}
      <button
        class="icon-button"
        type="button"
        aria-label={i18nManager.t('dashboard.refresh')}
        title={i18nManager.t('dashboard.refresh')}
        disabled={isLoading}
        onclick={() => void loadSnapshot()}
      >
        <svg class={{ rotating: isLoading }} viewBox="0 0 24 24" aria-hidden="true">
          <path d="M20 11a8.1 8.1 0 0 0-15.5-2M4 4v5h5M4 13a8.1 8.1 0 0 0 15.5 2M20 20v-5h-5" />
        </svg>
      </button>
    </div>
  </header>

  {#if loadError || snapshot?.sync.lastError}
    <div class="error-banner" role="alert">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="9" />
        <path d="M12 7v6M12 17h.01" />
      </svg>
      <span>{loadError || snapshot?.sync.lastError}</span>
    </div>
  {/if}

  {#if isLoading && !snapshot}
    <section class="loading-state" aria-label={i18nManager.t('dashboard.loading')}>
      <span class="loading-ring"></span>
      <p>{i18nManager.t('dashboard.loading')}</p>
    </section>
  {:else if snapshot}
    <section class="overview" aria-label={i18nManager.t('dashboard.summary.label')}>
      <div class="summary-grid">
        <article>
          <span>{i18nManager.t('dashboard.summary.sessions')}</span>
          <strong>{formatInteger(snapshot.summary.sessionCount)}</strong>
        </article>
        <article>
          <span>{i18nManager.t('dashboard.session.turns')}</span>
          <strong>{formatInteger(totalTurnCount)}</strong>
        </article>
        <article>
          <span>{i18nManager.t('dashboard.summary.tokens')}</span>
          <strong>{formatCompactInteger(snapshot.summary.totalTokens)}</strong>
        </article>
        <article>
          <span>{i18nManager.t('dashboard.summary.skills')}</span>
          <strong>{formatInteger(snapshot.summary.skillInvocationCount)}</strong>
        </article>
      </div>

      <div class="outcome-overview">
        <div class="outcome-heading">
          <span>{i18nManager.t('dashboard.turns.label')}</span>
          <strong>{formatInteger(totalTurnCount)}</strong>
        </div>
        <div class="outcome-visual">
          <div
            class="outcome-bar"
            role="img"
            aria-label={outcomeDescription}
          >
            {#each outcomeSegments as segment (segment.key)}
              {#if segment.value > 0}
                <span
                  class={`segment ${segment.key}`}
                  style:--segment-width={outcomeWidth(segment.value)}
                  title={segment.label}
                ></span>
              {/if}
            {/each}
          </div>
          <div class="outcome-legend">
            {#each outcomeSegments as segment (segment.key)}
              <span class={`outcome ${segment.key}`}>
                <span class="legend-dot" aria-hidden="true"></span>
                {segment.label}
              </span>
            {/each}
          </div>
        </div>
      </div>
    </section>

    <section class="data-panel">
      <div class="panel-toolbar">
        <div class="view-tabs" role="tablist" aria-label={i18nManager.t('dashboard.views.label')}>
          <button
            id="analytics-sessions-tab"
            type="button"
            role="tab"
            data-testid="analytics-sessions-tab"
            aria-selected={activeView === 'sessions'}
            aria-controls="analytics-data-panel"
            tabindex={activeView === 'sessions' ? 0 : -1}
            class={{ active: activeView === 'sessions' }}
            onclick={() => setView('sessions')}
            onkeydown={handleViewKeydown}
          >
            {i18nManager.t('dashboard.views.sessions')}
            <span>{formatInteger(snapshot.sessions.length)}</span>
          </button>
          <button
            id="analytics-skills-tab"
            type="button"
            role="tab"
            data-testid="analytics-skills-tab"
            aria-selected={activeView === 'skills'}
            aria-controls="analytics-data-panel"
            tabindex={activeView === 'skills' ? 0 : -1}
            class={{ active: activeView === 'skills' }}
            onclick={() => setView('skills')}
            onkeydown={handleViewKeydown}
          >
            {i18nManager.t('dashboard.views.skills')}
            <span>{formatInteger(snapshot.skills.length)}</span>
          </button>
        </div>

        <div class="toolbar-actions">
          {#if activeView === 'sessions'}
            <div
              class="archive-filter"
              role="group"
              aria-label={i18nManager.t('dashboard.filter.label')}
            >
              {#each ['all', 'current', 'archived'] as filter (filter)}
                <button
                  type="button"
                  aria-pressed={sessionFilter === filter}
                  class={{ active: sessionFilter === filter }}
                  onclick={() => setSessionFilter(filter as SessionFilter)}
                >
                  {i18nManager.t(`dashboard.filter.${filter}`)}
                </button>
              {/each}
            </div>
          {/if}
          <label class="search">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="11" cy="11" r="7" />
              <path d="m20 20-4-4" />
            </svg>
            <span class="sr-only">{i18nManager.t('dashboard.search.label')}</span>
            <input
              type="search"
              value={query}
              oninput={handleQueryInput}
              aria-label={i18nManager.t('dashboard.search.label')}
              placeholder={activeView === 'sessions'
                ? i18nManager.t('dashboard.search.sessions')
                : i18nManager.t('dashboard.search.skills')}
            />
          </label>
        </div>
      </div>

      <div
        id="analytics-data-panel"
        class="table-scroll"
        role="tabpanel"
        aria-labelledby={`analytics-${activeView}-tab`}
      >
        {#if activeView === 'sessions'}
          {#if visibleSessions.length > 0}
            <table class="sessions-table">
              <caption class="sr-only">{i18nManager.t('dashboard.views.sessions')}</caption>
              <thead>
                <tr>
                  <th>{i18nManager.t('dashboard.session.name')}</th>
                  <th>{i18nManager.t('dashboard.session.updated')}</th>
                  <th class="numeric">{i18nManager.t('dashboard.session.duration')}</th>
                  <th class="numeric">{i18nManager.t('dashboard.session.tokens')}</th>
                  <th class="numeric">{i18nManager.t('dashboard.session.turns')}</th>
                  <th class="numeric">{i18nManager.t('dashboard.session.skills')}</th>
                  <th>{i18nManager.t('dashboard.session.status')}</th>
                </tr>
              </thead>
              <tbody>
                {#each pagedSessions as session (session.id)}
                  <tr>
                    <td class="primary-cell">
                      <span class="session-title selectable-text" title={session.title}>{sessionName(session)}</span>
                      <span class="session-meta">
                        {session.projectName || i18nManager.t('dashboard.session.unknown_project')}
                        {#if session.archived}
                          <span class="archive-label">{i18nManager.t('dashboard.filter.archived')}</span>
                        {/if}
                      </span>
                    </td>
                    <td class="muted-cell">{formatDate(session.updatedAtMs ?? session.createdAtMs)}</td>
                    <td class="numeric" title={session.wallDurationMs === null ? '' : i18nManager.t('dashboard.session.wall_duration', { duration: formatDuration(session.wallDurationMs) })}>
                      {formatDuration(session.observedDurationMs)}
                    </td>
                    <td class="numeric mono">{formatCompactInteger(session.tokensUsed)}</td>
                    <td class="numeric mono">{formatInteger(session.turnCount)}</td>
                    <td class="numeric mono">{formatInteger(session.skillInvocationCount)}</td>
                    <td><span class={`status-badge ${session.status}`}>{statusLabel(session.status)}</span></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <div class="empty-state">
              <p>{snapshot.sessions.length === 0 ? i18nManager.t('dashboard.empty.sessions') : i18nManager.t('dashboard.empty.filtered')}</p>
            </div>
          {/if}
        {:else if visibleSkills.length > 0}
          <table class="skills-table">
            <caption class="sr-only">{i18nManager.t('dashboard.views.skills')}</caption>
            <thead>
              <tr>
                <th>{i18nManager.t('dashboard.skill.name')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.invocations')}</th>
                <th>{i18nManager.t('dashboard.skill.outcomes')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.success_rate')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.average_duration')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.max_duration')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.average_tokens')}</th>
                <th class="numeric">{i18nManager.t('dashboard.skill.max_tokens')}</th>
              </tr>
            </thead>
            <tbody>
              {#each pagedSkills as skill (skill.name)}
                <tr>
                  <td class="skill-name selectable-text">{skill.name}</td>
                  <td class="numeric mono"><strong>{formatInteger(skill.invocationCount)}</strong></td>
                  <td>
                    <span
                      class="outcome-counts"
                      aria-label={outcomeLabel(skill)}
                      title={outcomeLabel(skill)}
                    >
                      <span class="succeeded">{skill.succeededCount}</span>
                      <span class="failed">{skill.failedCount}</span>
                      <span class="cancelled">{skill.cancelledCount}</span>
                      <span class="unknown">{skill.unknownCount}</span>
                    </span>
                  </td>
                  <td class="numeric mono">{skill.successRate === null ? '—' : `${Math.round(skill.successRate * 100)}%`}</td>
                  <td class="numeric">{formatDuration(skill.averageDurationMs)}</td>
                  <td class="numeric">{formatDuration(skill.maxDurationMs)}</td>
                  <td class="numeric mono">{skill.averageTokens === null ? '—' : formatCompactInteger(skill.averageTokens)}</td>
                  <td class="numeric mono">{skill.maxTokens === null ? '—' : formatCompactInteger(skill.maxTokens)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <div class="empty-state">
            <p>{snapshot.skills.length === 0 ? i18nManager.t('dashboard.empty.skills') : i18nManager.t('dashboard.empty.filtered')}</p>
          </div>
        {/if}
      </div>

      <footer class="panel-footer">
        <p>{i18nManager.t('dashboard.method')}</p>
        <div class="footer-status">
          <span class="result-count">
            {activeView === 'sessions'
              ? `${formatInteger(visibleSessions.length)} / ${formatInteger(snapshot.sessions.length)}`
              : `${formatInteger(visibleSkills.length)} / ${formatInteger(snapshot.skills.length)}`}
          </span>
          {#if snapshot.sync.status === 'syncing'}
            <span>{i18nManager.t('dashboard.sync.processed', { count: formatInteger(snapshot.sync.processedRecords) })}</span>
          {:else if snapshot.sync.updatedAtMs}
            <span>{i18nManager.t('dashboard.sync.updated', { date: formatDate(snapshot.sync.updatedAtMs) })}</span>
          {/if}
          {#if activePageCount > 1}
            <nav class="pagination" aria-label={i18nManager.t('dashboard.pagination.label')}>
              <button
                class="page-button"
                type="button"
                aria-label={i18nManager.t('dashboard.pagination.previous')}
                disabled={activePage === 1}
                onclick={() => goToPage(activeView, activePage - 1)}
              >
                <span aria-hidden="true">‹</span>
              </button>
              <span class="page-number" aria-live="polite">
                {i18nManager.t('dashboard.pagination.page', {
                  page: formatInteger(activePage),
                  total: formatInteger(activePageCount)
                })}
              </span>
              <button
                class="page-button"
                type="button"
                aria-label={i18nManager.t('dashboard.pagination.next')}
                disabled={activePage === activePageCount}
                onclick={() => goToPage(activeView, activePage + 1)}
              >
                <span aria-hidden="true">›</span>
              </button>
            </nav>
          {/if}
        </div>
      </footer>
    </section>
  {/if}

  <AboutDialog />
</main>

<style>
  .dashboard {
    --dash-canvas: #f5f7fb;
    --dash-surface: #ffffff;
    --dash-surface-muted: #f8f9fc;
    --dash-surface-hover: #f4f6fb;
    --dash-text: #172033;
    --dash-text-muted: #667085;
    --dash-text-subtle: #667085;
    --dash-border: #e2e7f0;
    --dash-border-strong: #cbd3e1;
    --dash-accent: #315efb;
    --dash-accent-soft: #eef2ff;
    --dash-focus: rgb(49 94 251 / 24%);
    --dash-success: #17824b;
    --dash-success-soft: #eaf8f0;
    --dash-danger: #c83737;
    --dash-danger-soft: #fff0f0;
    --dash-warning: #ad6500;
    --dash-warning-soft: #fff7e6;
    --dash-info: #2867d6;
    --dash-info-soft: #edf4ff;
    --dash-neutral: #748095;
    --dash-neutral-soft: #f0f2f6;
    --dash-shadow: 0 1px 2px rgb(16 24 40 / 4%), 0 8px 24px rgb(16 24 40 / 4%);
    box-sizing: border-box;
    min-height: 100%;
    padding: 26px 30px 38px;
    color: var(--dash-text);
    background:
      linear-gradient(180deg, rgb(49 94 251 / 2.5%) 0, transparent 220px),
      var(--dash-canvas);
  }

  :global(html.dark) .dashboard {
    --dash-canvas: #0c111d;
    --dash-surface: #121927;
    --dash-surface-muted: #161e2e;
    --dash-surface-hover: #192234;
    --dash-text: #f2f5fb;
    --dash-text-muted: #9da8bb;
    --dash-text-subtle: #8f9bb0;
    --dash-border: #263044;
    --dash-border-strong: #3a465e;
    --dash-accent: #6f8fff;
    --dash-accent-soft: rgb(79 111 255 / 14%);
    --dash-focus: rgb(111 143 255 / 30%);
    --dash-success: #58d18f;
    --dash-success-soft: rgb(39 174 96 / 13%);
    --dash-danger: #ff8585;
    --dash-danger-soft: rgb(239 68 68 / 13%);
    --dash-warning: #f4b960;
    --dash-warning-soft: rgb(245 158 11 / 13%);
    --dash-info: #78a6ff;
    --dash-info-soft: rgb(59 130 246 / 13%);
    --dash-neutral: #9aa6ba;
    --dash-neutral-soft: rgb(148 163 184 / 11%);
    --dash-shadow: 0 1px 2px rgb(0 0 0 / 20%), 0 12px 32px rgb(0 0 0 / 15%);
    background:
      linear-gradient(180deg, rgb(88 119 255 / 4%) 0, transparent 240px),
      var(--dash-canvas);
  }

  .dashboard-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    max-width: 1440px;
    margin: 0 auto;
  }

  .heading-copy {
    min-width: 0;
  }

  .context-line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 7px;
  }

  .source-mark {
    display: grid;
    width: 22px;
    height: 22px;
    place-items: center;
    border: 1px solid color-mix(in srgb, var(--dash-accent) 22%, var(--dash-border));
    border-radius: 6px;
    color: var(--dash-accent);
    background: var(--dash-accent-soft);
  }

  .source-mark svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
  }

  .eyebrow {
    margin: 0;
    color: var(--dash-text-muted);
    font-size: 10px;
    font-weight: 680;
    line-height: 1.4;
    letter-spacing: 0.075em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    color: var(--dash-text);
    font-size: 24px;
    font-weight: 680;
    line-height: 1.2;
    letter-spacing: -0.03em;
  }

  .subtitle {
    max-width: 720px;
    margin: 6px 0 0;
    color: var(--dash-text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .sync-area {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
  }

  .sync-state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 1px solid var(--dash-border);
    border-radius: 7px;
    color: var(--dash-text-muted);
    background: var(--dash-surface);
    font-size: 11px;
    font-weight: 550;
    white-space: nowrap;
  }

  .sync-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--dash-success);
    box-shadow: 0 0 0 3px var(--dash-success-soft);
  }

  .sync-state.syncing .sync-dot {
    background: var(--dash-info);
    box-shadow: 0 0 0 3px var(--dash-info-soft);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .sync-state.error .sync-dot {
    background: var(--dash-danger);
    box-shadow: 0 0 0 3px var(--dash-danger-soft);
  }

  .sync-state.idle .sync-dot {
    background: var(--dash-neutral);
    box-shadow: 0 0 0 3px var(--dash-neutral-soft);
  }

  .icon-button {
    display: inline-grid;
    width: 30px;
    height: 30px;
    padding: 0;
    place-items: center;
    border: 1px solid var(--dash-border);
    border-radius: 7px;
    color: var(--dash-text-muted);
    background: var(--dash-surface);
    cursor: pointer;
    transition: border-color 160ms ease, color 160ms ease, background-color 160ms ease;
  }

  .icon-button:hover:not(:disabled) {
    color: var(--dash-accent);
    border-color: color-mix(in srgb, var(--dash-accent) 48%, var(--dash-border));
    background: var(--dash-accent-soft);
  }

  .icon-button:focus-visible {
    outline: 2px solid var(--dash-focus);
    outline-offset: 2px;
  }

  .icon-button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .icon-button svg,
  .error-banner svg,
  .search svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .rotating {
    animation: rotate 0.9s linear infinite;
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 9px;
    max-width: 1440px;
    box-sizing: border-box;
    margin: 18px auto 0;
    padding: 9px 12px;
    border: 1px solid color-mix(in srgb, var(--dash-danger) 28%, transparent);
    border-radius: 7px;
    color: var(--dash-danger);
    background: var(--dash-danger-soft);
    font-size: 12px;
  }

  .overview {
    max-width: 1440px;
    margin: 22px auto 0;
    overflow: hidden;
    border: 1px solid var(--dash-border);
    border-radius: 10px;
    background: var(--dash-surface);
    box-shadow: var(--dash-shadow);
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .summary-grid article {
    display: grid;
    min-width: 0;
    grid-template-rows: auto auto;
    padding: 16px 18px 15px;
    border-right: 1px solid var(--dash-border);
  }

  .summary-grid article:last-child {
    border-right: 0;
  }

  .summary-grid span {
    color: var(--dash-text-muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.065em;
    text-transform: uppercase;
  }

  .summary-grid strong {
    margin-top: 6px;
    overflow: hidden;
    color: var(--dash-text);
    font-size: 25px;
    font-variant-numeric: tabular-nums;
    font-weight: 690;
    line-height: 1.15;
    letter-spacing: -0.035em;
    text-overflow: ellipsis;
  }

  .outcome-overview {
    display: grid;
    grid-template-columns: 176px minmax(0, 1fr);
    align-items: center;
    gap: 20px;
    min-height: 76px;
    padding: 12px 18px;
    border-top: 1px solid var(--dash-border);
    background: var(--dash-surface-muted);
  }

  .outcome-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding-right: 18px;
    border-right: 1px solid var(--dash-border);
    color: var(--dash-text-muted);
    font-size: 11px;
  }

  .outcome-heading strong {
    color: var(--dash-text);
    font-size: 15px;
    font-variant-numeric: tabular-nums;
    font-weight: 680;
  }

  .outcome-visual {
    display: grid;
    min-width: 0;
    gap: 10px;
  }

  .outcome-bar {
    display: flex;
    width: 100%;
    height: 7px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--dash-neutral-soft);
  }

  .segment {
    width: var(--segment-width);
    min-width: 1px;
  }

  .segment.succeeded,
  .outcome.succeeded .legend-dot {
    background: var(--dash-success);
  }

  .segment.failed,
  .outcome.failed .legend-dot {
    background: var(--dash-danger);
  }

  .segment.cancelled,
  .outcome.cancelled .legend-dot {
    background: var(--dash-warning);
  }

  .segment.active,
  .outcome.active .legend-dot {
    background: var(--dash-info);
  }

  .segment.unknown,
  .outcome.unknown .legend-dot {
    background: var(--dash-neutral);
  }

  .outcome-legend {
    display: flex;
    min-width: 0;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
  }

  .outcome {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--dash-text-muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .legend-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .succeeded {
    color: var(--dash-success);
  }

  .failed {
    color: var(--dash-danger);
  }

  .cancelled {
    color: var(--dash-warning);
  }

  .outcome.active {
    color: var(--dash-info);
  }

  .outcome.unknown,
  .outcome-counts .unknown {
    color: var(--dash-neutral);
  }

  .data-panel {
    max-width: 1440px;
    margin: 16px auto 0;
    overflow: hidden;
    border: 1px solid var(--dash-border);
    border-radius: 10px;
    background: var(--dash-surface);
    box-shadow: var(--dash-shadow);
  }

  .panel-toolbar {
    display: flex;
    min-height: 52px;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 0 14px;
    border-bottom: 1px solid var(--dash-border);
  }

  .view-tabs,
  .archive-filter,
  .toolbar-actions {
    display: flex;
    align-items: center;
  }

  .view-tabs {
    align-self: stretch;
    gap: 4px;
    padding: 8px 0;
  }

  .view-tabs button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    color: var(--dash-text-muted);
    background: transparent;
    font: inherit;
    font-size: 11px;
    font-weight: 630;
    cursor: pointer;
    transition: color 160ms ease, background-color 160ms ease;
  }

  .view-tabs button:hover {
    color: var(--dash-text);
    background: var(--dash-surface-hover);
  }

  .view-tabs button.active {
    color: var(--dash-accent);
    background: var(--dash-accent-soft);
  }

  .view-tabs button:focus-visible,
  .archive-filter button:focus-visible {
    outline: 2px solid var(--dash-focus);
    outline-offset: 1px;
  }

  .view-tabs button span {
    min-width: 18px;
    box-sizing: border-box;
    padding: 1px 5px;
    border-radius: 8px;
    color: var(--dash-text-subtle);
    background: var(--dash-surface-muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    font-weight: 620;
    text-align: center;
  }

  .view-tabs button.active span {
    color: var(--dash-accent);
    background: color-mix(in srgb, var(--dash-accent) 10%, var(--dash-surface));
  }

  .toolbar-actions {
    gap: 9px;
  }

  .archive-filter {
    padding: 2px;
    border: 1px solid var(--dash-border);
    border-radius: 6px;
    background: var(--dash-surface-muted);
  }

  .archive-filter button {
    height: 25px;
    padding: 0 9px;
    border: 0;
    border-radius: 4px;
    color: var(--dash-text-muted);
    background: transparent;
    font: inherit;
    font-size: 10px;
    cursor: pointer;
    transition: color 160ms ease, background-color 160ms ease;
  }

  .archive-filter button:hover {
    color: var(--dash-text);
  }

  .archive-filter button.active {
    color: var(--dash-text);
    background: var(--dash-surface);
    box-shadow: 0 1px 2px rgb(15 23 42 / 10%);
  }

  .search {
    display: flex;
    width: 228px;
    height: 31px;
    box-sizing: border-box;
    align-items: center;
    gap: 7px;
    padding: 0 9px;
    border: 1px solid var(--dash-border);
    border-radius: 6px;
    color: var(--dash-text-subtle);
    background: var(--dash-surface-muted);
    transition: border-color 160ms ease, box-shadow 160ms ease, background-color 160ms ease;
  }

  .search:focus-within {
    border-color: var(--dash-accent);
    background: var(--dash-surface);
    box-shadow: 0 0 0 3px var(--dash-focus);
  }

  .search input {
    width: 100%;
    min-width: 0;
    padding: 0;
    outline: 0;
    border: 0;
    color: var(--dash-text);
    background: transparent;
    font: inherit;
    font-size: 11px;
  }

  .search input::placeholder {
    color: var(--dash-text-subtle);
  }

  .table-scroll {
    min-height: 300px;
    overflow: auto;
    scrollbar-color: var(--dash-border-strong) transparent;
  }

  table {
    width: 100%;
    table-layout: fixed;
    border-spacing: 0;
    border-collapse: collapse;
    color: var(--dash-text);
    font-size: 11px;
  }

  th,
  td {
    height: 44px;
    box-sizing: border-box;
    padding: 6px 14px;
    border-bottom: 1px solid var(--dash-border);
    text-align: left;
    white-space: nowrap;
  }

  th {
    position: sticky;
    z-index: 1;
    top: 0;
    height: 36px;
    color: var(--dash-text-muted);
    background: var(--dash-surface-muted);
    font-size: 9px;
    font-weight: 720;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr:hover {
    background: var(--dash-surface-hover);
  }

  .sessions-table {
    min-width: 940px;
  }

  .skills-table {
    min-width: 980px;
  }

  .numeric {
    text-align: right;
  }

  .mono {
    font-variant-numeric: tabular-nums;
  }

  .primary-cell {
    width: 34%;
    max-width: 420px;
  }

  .session-title,
  .session-meta {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .session-title {
    color: var(--dash-text);
    font-weight: 630;
  }

  .session-meta,
  .muted-cell {
    margin-top: 2px;
    color: var(--dash-text-muted);
    font-size: 10px;
  }

  .archive-label {
    margin-left: 7px;
    padding: 1px 5px;
    border: 1px solid var(--dash-border);
    border-radius: 4px;
    color: var(--dash-text-subtle);
    background: var(--dash-surface-muted);
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 7px;
    border-radius: 5px;
    color: var(--dash-neutral);
    background: var(--dash-neutral-soft);
    font-size: 9px;
    font-weight: 650;
  }

  .status-badge::before {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
    content: "";
  }

  .status-badge.succeeded {
    color: var(--dash-success);
    background: var(--dash-success-soft);
  }

  .status-badge.failed {
    color: var(--dash-danger);
    background: var(--dash-danger-soft);
  }

  .status-badge.cancelled {
    color: var(--dash-warning);
    background: var(--dash-warning-soft);
  }

  .status-badge.in_progress {
    color: var(--dash-info);
    background: var(--dash-info-soft);
  }

  .skill-name {
    min-width: 160px;
    color: var(--dash-text);
    font-weight: 640;
  }

  .outcome-counts {
    display: inline-flex;
    gap: 4px;
    font-variant-numeric: tabular-nums;
  }

  .outcome-counts span {
    min-width: 15px;
    padding: 3px 5px;
    border-radius: 4px;
    text-align: center;
  }

  .outcome-counts .succeeded {
    background: var(--dash-success-soft);
  }

  .outcome-counts .failed {
    background: var(--dash-danger-soft);
  }

  .outcome-counts .cancelled {
    background: var(--dash-warning-soft);
  }

  .outcome-counts .unknown {
    background: var(--dash-neutral-soft);
  }

  .panel-footer {
    display: flex;
    min-height: 38px;
    box-sizing: border-box;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 14px;
    border-top: 1px solid var(--dash-border);
    color: var(--dash-text-subtle);
    background: var(--dash-surface-muted);
    font-size: 9px;
    line-height: 1.4;
  }

  .panel-footer p {
    max-width: 900px;
    margin: 0;
  }

  .panel-footer span {
    white-space: nowrap;
  }

  .footer-status {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 12px;
  }

  .pagination {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .page-button {
    display: inline-grid;
    width: 24px;
    height: 24px;
    padding: 0;
    place-items: center;
    border: 1px solid var(--dash-border);
    border-radius: 5px;
    color: var(--dash-text-muted);
    background: var(--dash-surface);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }

  .page-button:hover:not(:disabled) {
    color: var(--dash-accent);
    border-color: color-mix(in srgb, var(--dash-accent) 48%, var(--dash-border));
    background: var(--dash-accent-soft);
  }

  .page-button:focus-visible {
    outline: 2px solid var(--dash-focus);
    outline-offset: 1px;
  }

  .page-button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .page-number {
    min-width: 68px;
    color: var(--dash-text-muted);
    font-variant-numeric: tabular-nums;
    text-align: center;
    white-space: nowrap;
  }

  .result-count {
    padding: 2px 6px;
    border: 1px solid var(--dash-border);
    border-radius: 4px;
    color: var(--dash-text-muted);
    background: var(--dash-surface);
    font-variant-numeric: tabular-nums;
  }

  .empty-state,
  .loading-state {
    display: grid;
    min-height: 300px;
    place-content: center;
    color: var(--dash-text-muted);
    font-size: 12px;
    text-align: center;
  }

  .loading-state {
    gap: 12px;
  }

  .loading-state p,
  .empty-state p {
    margin: 0;
  }

  .loading-ring {
    width: 20px;
    height: 20px;
    margin: 0 auto;
    border: 2px solid var(--dash-border);
    border-top-color: var(--dash-accent);
    border-radius: 50%;
    animation: rotate 0.9s linear infinite;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    border: 0;
    white-space: nowrap;
  }

  @keyframes rotate {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }

  @media (max-width: 1080px) {
    .dashboard {
      padding: 22px 22px 32px;
    }

    .summary-grid article {
      padding-inline: 14px;
    }

    .outcome-overview {
      grid-template-columns: 150px minmax(0, 1fr);
      gap: 14px;
      padding-inline: 14px;
    }

    .toolbar-actions {
      gap: 6px;
    }

    .search {
      width: 184px;
    }
  }

  @media (max-width: 980px) {
    .dashboard-header {
      align-items: flex-start;
    }

    .summary-grid article {
      padding-inline: 12px;
    }

    .panel-toolbar {
      align-items: stretch;
      flex-direction: column;
      gap: 0;
      padding-top: 0;
      padding-bottom: 10px;
    }

    .view-tabs {
      height: 42px;
    }

    .toolbar-actions {
      justify-content: space-between;
    }

    .search {
      width: min(260px, 42vw);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .icon-button,
    .view-tabs button,
    .archive-filter button,
    .search {
      transition: none;
    }

    .rotating,
    .loading-ring,
    .sync-state.syncing .sync-dot {
      animation: none;
    }
  }
</style>
