<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { formatApiError, triggerPoll } from '#lib/api.ts';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import {
    describeOperationalMode,
    extractOperationalMode,
    isSiteChangedVisible,
  } from '#lib/format.ts';
  import type { AgentSnapshot, UiStatePayload } from '#lib/schemas.ts';
  import type { WsStatus } from '#lib/ui-state.svelte.ts';

  interface Props {
    snapshot: AgentSnapshot | null;
    wsStatus: WsStatus;
    pollInProgress?: boolean;
    isSyncing?: boolean;
    onPollApplied?: (next: UiStatePayload) => void;
    onNotify?: (kind: 'ok' | 'info' | 'error', text: string) => void;
  }

  let {
    snapshot,
    wsStatus,
    pollInProgress = false,
    isSyncing = false,
    onPollApplied,
    onNotify,
  }: Props = $props();

  let triggering = $state(false);

  let modeInfo = $derived(
    snapshot ? describeOperationalMode(extractOperationalMode(snapshot)) : null,
  );
  let changedCount = $derived(
    snapshot ? snapshot.sites.filter(isSiteChangedVisible).length : 0,
  );
  let decisionsCount = $derived(snapshot ? snapshot.inference.decisions.length : 0);

  let pathname = $derived(page.url.pathname);
  let isIndexActive = $derived(pathname === '/' || pathname.startsWith('/article/'));
  let isDssActive = $derived(pathname === '/dss');
  let isConfActive = $derived(pathname === '/conf');
  let isGuideActive = $derived(pathname === '/guide');

  let wsLabel = $derived.by(() => {
    if (wsStatus === 'connected') return isSyncing ? 'Синхронізація…' : 'Live';
    if (wsStatus === 'connecting') return 'Підключення…';
    return 'Перепідключення…';
  });

  let wsTooltip = $derived.by(() => {
    if (wsStatus === 'connected') {
      return 'Відкрито WebSocket-з’єднання (/api/ws). Інтерфейс автоматично оновлюється одразу після завершення кожної ітерації Web-агента.';
    }
    if (wsStatus === 'connecting') {
      return 'Встановлюється WebSocket-з’єднання (/api/ws) для отримання миттєвих оновлень від Web-агента.';
    }
    return 'З’єднання WebSocket перервано. Виконується автоматичне перепідключення з експоненційною затримкою.';
  });

  async function handlePoll() {
    triggering = true;
    try {
      const updated = await triggerPoll();
      onPollApplied?.(updated);
      onNotify?.(
        'ok',
        'Позачергове опитування Web-джерел запущено. Дані оновляться автоматично через WebSocket.',
      );
    } catch (err) {
      onNotify?.('error', formatApiError(err, 'Не вдалося запустити позачергове опитування.'));
    } finally {
      triggering = false;
    }
  }
</script>

<header class="site-header">
  <div class="header-inner">
    <a class="brand" href={resolve('/')}>
      <span class="brand-title">
        <span class="brand-dot" aria-hidden="true"></span>
        Web-агент та ІСППР
      </span>
      <span class="brand-sub">Моніторинг Web-джерел і підтримка рішень</span>
    </a>

    <nav class="header-nav" aria-label="Основна навігація">
      <a href={resolve('/')} class={['nav-link', isIndexActive && 'active']}>
        Огляд і моніторинг
        {#if changedCount > 0}
          <span class="nav-count num" title="Непереглянуті зміни сайтів">{changedCount}</span>
        {/if}
      </a>

      <a href={resolve('/dss')} class={['nav-link', isDssActive && 'active']}>
        Рішення ІСППР
        {#if decisionsCount > 0}
          <span class="nav-count num" title="Активні рішення ІСППР">{decisionsCount}</span>
        {/if}
      </a>

      <a href={resolve('/conf')} class={['nav-link', isConfActive && 'active']}>
        Конфігурація
      </a>

      <a href={resolve('/guide')} class={['nav-link', isGuideActive && 'active']}>
        Інструкція
      </a>

      <a class="nav-link api-link" href="/api/state" target="_blank" rel="noopener">
        JSON API ↗
      </a>
    </nav>

    <div class="header-actions">
      <Tooltip
        title="Канал реального часу (WebSocket)"
        text={wsTooltip}
        position="bottom"
        align="end"
        dark
      >
        <button type="button" class={['ws-chip', `ws-${wsStatus}`]}>
          <span class="ws-dot" aria-hidden="true"></span>
          <span>{wsLabel}</span>
        </button>
      </Tooltip>

      {#if modeInfo}
        <Tooltip
          title={`Операційний режим (${modeInfo.code})`}
          text="Визначається автоматично машиною прямого виводу ІСППР на основі фактів про CVE, зміни HTTP-сайтів та курси валют НБУ."
          position="bottom"
          align="end"
          dark
        >
          <a href={resolve('/dss')} class={['mode-chip', `tone-${modeInfo.tone}`]}>
            <span class="dot" aria-hidden="true"></span>
            <span>{modeInfo.title}</span>
          </a>
        </Tooltip>
      {:else}
        <span class="mode-chip loading-chip">
          <span class="mini-spin" aria-hidden="true"></span>
          <span>Завантаження…</span>
        </span>
      {/if}

      <button
        type="button"
        class="btn poll-btn"
        disabled={triggering || pollInProgress}
        onclick={handlePoll}
      >
        {#if triggering || pollInProgress}
          <span class="btn-spin" aria-hidden="true"></span>
          Опитування…
        {:else}
          ↻ Оновити зараз
        {/if}
      </button>
    </div>
  </div>
</header>

<style>
  .site-header {
    background: linear-gradient(180deg, #0f172a 0%, #16213a 100%);
    color: #f8fafc;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .header-inner {
    max-width: 1320px;
    margin: 0 auto;
    padding: 0.78rem 1.5rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.6rem;
  }

  .brand {
    display: flex;
    flex-direction: column;
    gap: 0.08rem;
    text-decoration: none;
    color: inherit;
    flex-shrink: 0;
  }

  .brand:hover {
    color: #ffffff;
  }

  .brand-title {
    font-size: 1.02rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    display: flex;
    align-items: center;
    gap: 0.45rem;
    white-space: nowrap;
  }

  .brand-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #38bdf8;
    box-shadow: 0 0 0 3px rgba(56, 189, 248, 0.25);
    flex-shrink: 0;
  }

  .brand-sub {
    font-size: 0.72rem;
    color: #94a3b8;
    white-space: nowrap;
  }

  .header-nav {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.18rem;
  }

  .nav-link {
    display: inline-flex;
    align-items: center;
    gap: 0.34rem;
    color: #cbd5e1;
    text-decoration: none;
    padding: 0.38rem 0.58rem;
    border-radius: var(--radius-sm);
    font-size: 0.83rem;
    font-weight: 500;
    white-space: nowrap;
    transition: background 0.15s, color 0.15s;
  }

  .nav-link:hover {
    background: rgba(255, 255, 255, 0.09);
    color: #ffffff;
  }

  .nav-link.active {
    background: var(--accent);
    color: #ffffff;
  }

  .api-link {
    color: #94a3b8;
    font-size: 0.78rem;
  }

  .nav-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.18rem;
    height: 1.18rem;
    padding: 0 0.32rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.16);
    font-size: 0.7rem;
    font-weight: 700;
  }

  .nav-link.active .nav-count {
    background: rgba(255, 255, 255, 0.25);
  }

  .header-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.42rem;
  }

  .ws-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.38rem;
    padding: 0.3rem 0.56rem;
    border-radius: 999px;
    font-size: 0.74rem;
    font-weight: 600;
    font-family: inherit;
    cursor: help;
    background: rgba(255, 255, 255, 0.07);
    color: #cbd5e1;
    border: 1px solid rgba(255, 255, 255, 0.12);
    white-space: nowrap;
  }

  .ws-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .ws-connected .ws-dot {
    background: #10b981;
    box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.28);
  }

  .ws-connected {
    color: #a7f3d0;
    border-color: rgba(16, 185, 129, 0.32);
  }

  .ws-connecting .ws-dot {
    background: #38bdf8;
    animation: pulse-dot 1.2s ease-in-out infinite;
  }

  .ws-reconnecting .ws-dot {
    background: #f59e0b;
    animation: pulse-dot 1s ease-in-out infinite;
  }

  .ws-reconnecting {
    color: #fde68a;
    border-color: rgba(245, 158, 11, 0.35);
  }

  .mode-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.64rem;
    border-radius: 999px;
    font-size: 0.76rem;
    font-weight: 600;
    background: rgba(255, 255, 255, 0.08);
    color: #e2e8f0;
    border: 1px solid rgba(255, 255, 255, 0.14);
    text-decoration: none;
    white-space: nowrap;
  }

  .mode-chip:hover {
    background: rgba(255, 255, 255, 0.14);
    color: #ffffff;
  }

  .loading-chip {
    color: #94a3b8;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tone-critical .dot {
    background: #ef4444;
    box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.32);
  }

  .tone-high .dot {
    background: #f97316;
    box-shadow: 0 0 0 3px rgba(249, 115, 22, 0.32);
  }

  .tone-medium .dot {
    background: #eab308;
  }

  .tone-ok .dot {
    background: #10b981;
  }

  .tone-info .dot {
    background: #38bdf8;
  }

  .mini-spin,
  .btn-spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: #ffffff;
    animation: hdr-spin 0.75s linear infinite;
    flex-shrink: 0;
  }

  .poll-btn {
    white-space: nowrap;
    padding: 0.4rem 0.76rem;
    font-size: 0.81rem;
  }

  @keyframes hdr-spin {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes pulse-dot {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }
</style>
