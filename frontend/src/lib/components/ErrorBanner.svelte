<script lang="ts">
  import { formatApiError } from '#lib/api.ts';

  interface Props {
    error: unknown;
    title?: string;
    onRetry?: () => void;
  }

  let { error, title = 'Не вдалося завантажити дані', onRetry }: Props = $props();

  let message = $derived(formatApiError(error));
</script>

<section class="panel error-card" role="alert">
  <div class="err-head">
    <span class="badge sev-critical">Помилка</span>
    <h2 class="err-title">{title}</h2>
  </div>
  <p class="err-msg">{message}</p>
  <p class="muted err-hint">
    Переконайтеся, що сервер <code>web-agent-server</code> запущений і доступний, після чого
    повторіть спробу.
  </p>
  {#if onRetry}
    <div class="err-actions">
      <button type="button" class="btn" onclick={onRetry}>↻ Спробувати ще раз</button>
    </div>
  {/if}
</section>

<style>
  .error-card {
    border-color: var(--critical-border);
    border-left: 5px solid var(--critical-solid);
    background: #fffafa;
  }

  .err-head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.45rem;
  }

  .err-title {
    margin: 0;
    font-size: 1.12rem;
    color: var(--critical-fg);
  }

  .err-msg {
    margin: 0 0 0.45rem;
    color: var(--text);
    font-weight: 600;
    font-size: 0.93rem;
  }

  .err-hint {
    margin: 0;
  }

  .err-actions {
    margin-top: 0.95rem;
  }
</style>
