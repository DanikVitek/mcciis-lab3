<script lang="ts">
  import ErrorBanner from '#lib/components/ErrorBanner.svelte';
  import LoadingSkeleton from '#lib/components/LoadingSkeleton.svelte';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import { groupLabelUk } from '#lib/format.ts';
  import { getNotify } from '#lib/notify-context.ts';
  import type { AppConfig, LogRotation } from '#lib/schemas.ts';
  import { getUiState } from '#lib/ui-state.svelte.ts';

  const notify = getNotify();
  const store = getUiState();

  let copied = $state(false);

  function rotationLabelUk(rot: LogRotation): string {
    switch (rot) {
      case 'hourly':
        return 'Щогодини';
      case 'daily':
        return 'Щодня';
      case 'never':
        return 'Без ротації';
    }
  }

  function computeBackoffSteps(config: AppConfig): number[] {
    const steps: number[] = [];
    let cur = Math.max(1, config.server.retry_policy.initial_backoff_secs);
    const max = Math.max(cur, config.server.retry_policy.max_backoff_secs);
    while (steps.length < 7) {
      steps.push(cur);
      if (cur >= max) break;
      cur = Math.min(max, cur * 2);
    }
    return steps;
  }

  async function copyToml(prettyToml: string) {
    try {
      await navigator.clipboard.writeText(prettyToml);
      copied = true;
      notify('ok', 'Конфігурацію TOML скопійовано до буфера обміну.');
      setTimeout(() => {
        copied = false;
      }, 2200);
    } catch {
      notify('error', 'Не вдалося скопіювати TOML (перевірте доступ до буфера обміну).');
    }
  }
</script>

<svelte:head>
  <title>Конфігурація — Інтелектуальний Web-агент та ІСППР</title>
</svelte:head>

{#snippet confContent(config: AppConfig, prettyToml: string)}
  {@const backoffSteps = computeBackoffSteps(config)}

  <!-- 1. Summary Grid -->
  <div class="summary-grid">
    <section class="panel summary-card">
      <h2 class="panel-title title-with-tip">
        Сервер та опитування
        <Tooltip
          title="Параметри планувальника"
          text="Агент виконує опитування кожні poll_interval_secs секунд із обмеженням паралелізму через Tokio Semaphore (max_concurrent_requests)."
          align="start"
        />
      </h2>
      <p class="panel-desc">Параметри HTTP API та фонового планувальника Web-агента.</p>
      <dl class="kv-list">
        <div>
          <dt>Адреса API-сервера</dt>
          <dd class="mono">{config.server.bind_addr}</dd>
        </div>
        <div>
          <dt>Інтервал опитування</dt>
          <dd class="mono num">{config.server.poll_interval_secs} с</dd>
        </div>
        <div>
          <dt>Таймаут HTTP-запиту</dt>
          <dd class="mono num">{config.server.request_timeout_secs} с</dd>
        </div>
        <div>
          <dt>Макс. паралельних запитів</dt>
          <dd class="mono num">{config.server.max_concurrent_requests}</dd>
        </div>
      </dl>
    </section>

    <section class="panel summary-card">
      <h2 class="panel-title title-with-tip">
        Відмовостійкість (Backoff / CB)
        <Tooltip
          title="Захист від збоїв джерел"
          text="Тимчасові мережеві збої збільшують затримку експоненційно (×2). Повторювані помилки парсингу після parse_max_retries спроб автоматично вимикають лише проблемне джерело."
          align="start"
        />
      </h2>
      <p class="panel-desc">
        Автоматичне вимкнення джерела при повторюваних помилках парсингу та експоненційна пауза при
        мережевих збоях.
      </p>
      <dl class="kv-list">
        <div>
          <dt>Поріг помилок парсингу</dt>
          <dd class="mono num">{config.server.retry_policy.parse_max_retries} спроби</dd>
        </div>
        <div>
          <dt>Початкова / макс. затримка</dt>
          <dd class="mono num">
            {config.server.retry_policy.initial_backoff_secs} с →
            {config.server.retry_policy.max_backoff_secs} с
          </dd>
        </div>
      </dl>
      <div class="backoff-label muted">Сходинки експоненційного backoff:</div>
      <div class="backoff-steps">
        {#each backoffSteps as sec, idx (`step-${idx}-${sec}`)}
          <span class="step-chip mono num">{sec}с</span>
          {#if idx < backoffSteps.length - 1}
            <span class="step-arrow" aria-hidden="true">→</span>
          {/if}
        {/each}
      </div>
    </section>

    <section class="panel summary-card">
      <h2 class="panel-title title-with-tip">
        ІСППР, НБУ та журналювання
        <Tooltip
          title="Збереження стану та логів"
          text="Стан агента зберігається на диск після кожного циклу (state_path), а системні події пишуться у неблокуючий ротований файл журналу."
          align="end"
        />
      </h2>
      <p class="panel-desc">Шляхи до бази знань, стану, ротації логів та інтеграції з API НБУ.</p>
      <dl class="kv-list">
        <div>
          <dt>База правил ІСППР</dt>
          <dd class="mono">{config.dss.rules_path}</dd>
        </div>
        <div>
          <dt>Файл збереження стану</dt>
          <dd class="mono">{config.dss.state_path}</dd>
        </div>
        <div>
          <dt>Журнал подій</dt>
          <dd class="mono">
            {config.logging.dir}/{config.logging.file_prefix}.* ({rotationLabelUk(
              config.logging.rotation,
            )}, макс. {config.logging.max_files})
          </dd>
        </div>
        <div>
          <dt>Валюти НБУ</dt>
          <dd>
            <span class={['badge', config.nbu_metrics.enabled ? 'badge-ok' : 'badge-group']}>
              {config.nbu_metrics.enabled ? 'Увімкнено' : 'Вимкнено'}
            </span>
            <span class="mono">{config.nbu_metrics.currencies.join(', ')}</span>
          </dd>
        </div>
      </dl>
    </section>
  </div>

  <!-- 2. Controlled HTTP/HTTPS Sites -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          Контрольовані HTTP/HTTPS-сайти ({config.http_sites.length})
          <Tooltip
            text="Список сторінок із секції [[http_sites]] файлу config.toml. Агент відстежує зміну їхнього вмісту між ітераціями."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Ресурси, для яких відстежуються зміни вмісту (SHA-256, ETag, Last-Modified, розмір тіла).
        </p>
      </div>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Назва ресурсу</th>
            <th>Категорія</th>
            <th>URL</th>
          </tr>
        </thead>
        <tbody>
          {#each config.http_sites as site (site.id)}
            <tr>
              <td class="mono">{site.id}</td>
              <td><strong>{site.name}</strong></td>
              <td><span class="badge badge-group">{groupLabelUk(site.category)}</span></td>
              <td class="mono url-cell">
                <a href={site.url} target="_blank" rel="noopener">{site.url} ↗</a>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <!-- 3. News Feeds & Weighted Keywords -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          Джерела новин (RSS/Atom) та зважені ключові слова ({config.news_feeds.length})
          <Tooltip
            title="Ваги ключових слів"
            text="Кожне ключове слово має вагу w (наприклад, ×3.0). Збіг у заголовку додає 2×w балів до рейтингу новини, а в тексті опису — 1×w."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Для кожної стрічки задано тематичну групу та словник ключових слів із вагами для
          обчислення рейтингу релевантності.
        </p>
      </div>
    </div>

    <div class="feeds-grid">
      {#each config.news_feeds as feed (feed.id)}
        <article class="feed-card">
          <div class="feed-head">
            <div>
              <h3 class="feed-name">{feed.name}</h3>
              <div class="mono muted">{feed.id}</div>
            </div>
            <span class="badge badge-group">{groupLabelUk(feed.group)}</span>
          </div>

          <div class="feed-url mono">
            <a href={feed.url} target="_blank" rel="noopener">{feed.url} ↗</a>
          </div>

          <div class="kw-label muted">Ключові слова та ваги ({feed.keywords.length}):</div>
          <div class="kw-list">
            {#each feed.keywords as kw (kw.word)}
              <span class="badge badge-kw">
                {kw.word}
                <span class="kw-w mono num">×{kw.weight.toFixed(1)}</span>
              </span>
            {/each}
          </div>
        </article>
      {/each}
    </div>
  </section>

  <!-- 4. Canonical TOML -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title">Канонічна конфігурація (TOML)</h2>
        <p class="panel-desc">
          Актуальний знімок завантаженої конфігурації <code>config.toml</code> у форматі TOML.
        </p>
      </div>
      <button
        type="button"
        class="btn btn-secondary btn-sm"
        onclick={() => void copyToml(prettyToml)}
      >
        {copied ? '✓ Скопійовано' : 'Копіювати TOML'}
      </button>
    </div>

    <details>
      <summary>
        Розгорнути повний текст <code>config.toml</code> ({prettyToml.split('\n').length} рядків)
      </summary>
      <pre class="toml-block"><code>{prettyToml}</code></pre>
    </details>
  </section>
{/snippet}

<svelte:boundary>
  {@const uiState = await store.load()}
  {@render confContent(uiState.config, uiState.pretty_toml)}

  {#snippet pending()}
    <LoadingSkeleton variant="conf" message="Завантаження конфігурації Web-агента…" />
  {/snippet}

  {#snippet failed(err, reset)}
    <ErrorBanner
      error={err}
      title="Не вдалося завантажити конфігурацію"
      onRetry={() => {
        store.retry();
        reset();
      }}
    />
  {/snippet}
</svelte:boundary>

<style>
  .title-with-tip {
    display: inline-flex;
    align-items: center;
    gap: 0.42rem;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 1.15rem;
  }

  .summary-card {
    display: flex;
    flex-direction: column;
  }

  .kv-list {
    margin: 0.85rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  .kv-list > div {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding-bottom: 0.45rem;
    border-bottom: 1px dashed var(--border);
  }

  .kv-list > div:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .kv-list dt {
    font-size: 0.76rem;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--muted);
  }

  .kv-list dd {
    margin: 0;
    font-size: 0.88rem;
    color: var(--text);
    word-break: break-word;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .backoff-label {
    margin-top: 0.75rem;
    margin-bottom: 0.35rem;
    font-size: 0.8rem;
  }

  .backoff-steps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
  }

  .step-chip {
    padding: 0.16rem 0.45rem;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--info-fg);
    border: 1px solid var(--accent-border);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .step-arrow {
    color: var(--muted);
    font-size: 0.78rem;
  }

  .url-cell {
    word-break: break-all;
  }

  .feeds-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.9rem;
  }

  .feed-card {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 1rem 1.1rem;
  }

  .feed-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.6rem;
    margin-bottom: 0.35rem;
  }

  .feed-name {
    margin: 0;
    font-size: 0.98rem;
  }

  .feed-url {
    font-size: 0.8rem;
    word-break: break-all;
    margin-bottom: 0.7rem;
  }

  .kw-label {
    font-size: 0.8rem;
    margin-bottom: 0.4rem;
  }

  .kw-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .kw-w {
    font-size: 0.72rem;
    opacity: 0.82;
  }

  .toml-block {
    margin: 0.75rem 0 0;
    padding: 1rem 1.15rem;
    background: #0f172a;
    color: #e2e8f0;
    border-radius: var(--radius-md);
    overflow-x: auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.82rem;
    line-height: 1.55;
  }

  @media (max-width: 960px) {
    .summary-grid,
    .feeds-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
