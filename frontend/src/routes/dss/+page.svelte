<script lang="ts">
  import { clearManualFacts, formatApiError, upsertManualFact } from '#lib/api.ts';
  import CfMeter from '#lib/components/CfMeter.svelte';
  import ErrorBanner from '#lib/components/ErrorBanner.svelte';
  import LoadingSkeleton from '#lib/components/LoadingSkeleton.svelte';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import {
    describeOperationalMode,
    extractOperationalMode,
    factDescriptionUk,
    formatDateTimeUk,
    formatFactValue,
    mergeAllFacts,
    operatorSymbol,
    originLabelUk,
    severityLabelUk,
  } from '#lib/format.ts';
  import { getNotify } from '#lib/notify-context.ts';
  import type {
    AgentSnapshot,
    Decision,
    Fact,
    FactOrigin,
    Rule,
    Severity,
  } from '#lib/schemas.ts';
  import { getUiState } from '#lib/ui-state.svelte.ts';

  const notify = getNotify();
  const store = getUiState();

  let severityFilter = $state<'all' | Severity>('all');
  let originFilter = $state<'all' | FactOrigin>('all');
  let factSearch = $state('');
  let ruleFilter = $state<'all' | 'fired' | 'idle'>('all');

  let factKey = $state('');
  let factValue = $state('');
  let factCf = $state(1.0);
  let formError = $state<string | null>(null);
  let submittingFact = $state(false);
  let clearingFacts = $state(false);

  const PRESETS = [
    {
      label: 'Кризовий сплеск CVE',
      hint: 'cve_mentions_count = 3 (CF 1.0)',
      key: 'cve_mentions_count',
      value: '3',
      cf: 1.0,
    },
    {
      label: 'Валютний тиск USD',
      hint: 'nbu_rate_usd = 46.5 (CF 1.0)',
      key: 'nbu_rate_usd',
      value: '46.5',
      cf: 1.0,
    },
    {
      label: 'Зміни на HTTP-сайтах',
      hint: 'http_changed_sites_count = 2 (CF 0.95)',
      key: 'http_changed_sites_count',
      value: '2',
      cf: 0.95,
    },
    {
      label: 'Критична кіберновина',
      hint: 'top_article_score = 12.0 (CF 0.9)',
      key: 'top_article_score',
      value: '12.0',
      cf: 0.9,
    },
  ] as const;

  function getSuggestedKeys(allFacts: Fact[], rules: Rule[]): string[] {
    const keys = new Set<string>();
    for (const f of allFacts) keys.add(f.key);
    for (const r of rules) {
      for (const c of r.conditions) keys.add(c.fact);
    }
    return Array.from(keys).sort();
  }

  function getFilteredFacts(allFacts: Fact[]): Fact[] {
    const q = factSearch.trim().toLowerCase();
    return allFacts.filter((f) => {
      if (originFilter !== 'all' && f.origin !== originFilter) return false;
      if (q) {
        const hay =
          `${f.key} ${factDescriptionUk(f.key)} ${formatFactValue(f.value)} ${f.source}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }

  function getFilteredRules(rules: Rule[], firedSet: Set<string>): Rule[] {
    return rules.filter((r) => {
      const fired = firedSet.has(r.id);
      if (ruleFilter === 'fired') return fired;
      if (ruleFilter === 'idle') return !fired;
      return true;
    });
  }

  function applyPreset(preset: (typeof PRESETS)[number]) {
    factKey = preset.key;
    factValue = preset.value;
    factCf = preset.cf;
    formError = null;
  }

  async function handleUpsertFact(event: SubmitEvent) {
    event.preventDefault();
    formError = null;
    submittingFact = true;
    try {
      const key = factKey.trim();
      const value = factValue.trim();
      const updated = await upsertManualFact({ key, value, cf: factCf });
      store.applyPayload(updated);
      notify('ok', `Факт «${key} = ${value}» застосовано, вивід ІСППР перераховано.`);
    } catch (err) {
      const msg = formatApiError(err, 'Перевірте коректність введеного факту.');
      formError = msg;
      notify('error', msg);
    } finally {
      submittingFact = false;
    }
  }

  async function handleClearFacts() {
    clearingFacts = true;
    formError = null;
    try {
      const updated = await clearManualFacts();
      store.applyPayload(updated);
      notify('info', 'Усі ручні факти очищено, відновлено стан Web-агента.');
    } catch (err) {
      notify('error', formatApiError(err, 'Не вдалося очистити ручні факти.'));
    } finally {
      clearingFacts = false;
    }
  }
</script>

<svelte:head>
  <title>Рішення ІСППР — Інтелектуальний Web-агент та ІСППР</title>
</svelte:head>

{#snippet decisionCard(dec: Decision, expanded: boolean)}
  <article class={['decision-card', `sev-border-${dec.severity}`]}>
    <div class="dec-top">
      <div class="dec-badges">
        <span class={['badge', `sev-${dec.severity}`]}>
          {severityLabelUk(dec.severity)}
        </span>
        <span class="mono muted">{dec.rule_id}</span>
        <span class="badge badge-group num">Пріоритет {dec.priority}</span>
        {#if dec.derived_fact}
          <span class="badge badge-kw mono">
            ⇒ {dec.derived_fact.key} = {formatFactValue(dec.derived_fact.value)}
          </span>
        {/if}
      </div>
      <CfMeter value={dec.cf} tone={dec.severity} label={`CF ${dec.cf.toFixed(2)}`} width="82px" />
    </div>

    <h3 class="dec-title">{dec.rule_name}</h3>
    <p class="dec-rec">{dec.recommendation}</p>

    <details open={expanded}>
      <summary>Ланцюжок доведення ({dec.explanation.length} умов)</summary>
      <div class="table-wrap trace-wrap">
        <table>
          <thead>
            <tr>
              <th>Факт робочої пам’яті</th>
              <th>Умова правила</th>
              <th>Фактичне значення</th>
              <th>Джерело</th>
              <th>CF факту</th>
            </tr>
          </thead>
          <tbody>
            {#each dec.explanation as tr, idx (`${dec.rule_id}-${tr.fact_key}-${idx}`)}
              <tr>
                <td>
                  <div class="mono">{tr.fact_key}</div>
                  <div class="muted">{factDescriptionUk(tr.fact_key)}</div>
                </td>
                <td class="mono">
                  {operatorSymbol(tr.operator)} {formatFactValue(tr.expected)}
                </td>
                <td class="mono num"><strong>{formatFactValue(tr.actual)}</strong></td>
                <td class="muted">{tr.fact_source}</td>
                <td>
                  <CfMeter value={tr.fact_cf} label={tr.fact_cf.toFixed(2)} width="56px" />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </details>
  </article>
{/snippet}

{#snippet ruleCard(rule: Rule, isFired: boolean)}
  <article class={['rule-card', isFired && 'rule-fired']}>
    <div class="rule-head">
      <div class="rule-badges">
        <span class="mono"><strong>{rule.id}</strong></span>
        <span class={['badge', `sev-${rule.severity}`]}>{severityLabelUk(rule.severity)}</span>
        <span class="badge badge-group num">P={rule.priority}</span>
        <span class="badge badge-group mono num">CF={rule.certainty_factor.toFixed(2)}</span>
      </div>
      <span class={['badge', isFired ? 'badge-ok' : 'badge-group']}>
        {isFired ? '✓ Спрацювало' : 'Неактивне'}
      </span>
    </div>

    <h3 class="rule-name">{rule.name}</h3>
    <p class="muted rule-desc">{rule.description}</p>

    <div class="rule-logic">
      <div class="logic-block">
        <span class="logic-kw">ЯКЩО</span>
        <div class="cond-list">
          {#each rule.conditions as cond, ci (`${rule.id}-c-${ci}`)}
            <span class="cond-pill mono">
              {cond.fact} {operatorSymbol(cond.op)} {formatFactValue(cond.value)}
            </span>
          {/each}
        </div>
      </div>

      <div class="logic-block">
        <span class="logic-kw">ТО</span>
        <div class="then-col">
          {#if rule.derived_fact}
            <span class="badge badge-kw mono">
              {rule.derived_fact.key} = {formatFactValue(rule.derived_fact.value)}
            </span>
          {/if}
          <span class="rule-rec">{rule.recommendation}</span>
        </div>
      </div>
    </div>
  </article>
{/snippet}

{#snippet dssContent(snapshot: AgentSnapshot, rules: Rule[])}
  {@const modeInfo = describeOperationalMode(extractOperationalMode(snapshot))}
  {@const allFacts = mergeAllFacts(snapshot)}
  {@const manualFactsList = Object.values(snapshot.manual_facts).sort((a, b) =>
    a.key.localeCompare(b.key),
  )}
  {@const firedSet = new Set(snapshot.inference.fired_rule_ids)}
  {@const suggestedKeys = getSuggestedKeys(allFacts, rules)}
  {@const filteredDecisions = snapshot.inference.decisions.filter(
    (d) => severityFilter === 'all' || d.severity === severityFilter,
  )}
  {@const filteredFacts = getFilteredFacts(allFacts)}
  {@const filteredRules = getFilteredRules(rules, firedSet)}

  <!-- 1. DSS Summary Banner -->
  <section class={['panel', 'dss-hero', `tone-${modeInfo.tone}`]}>
    <div class="hero-row">
      <div>
        <div class="eyebrow">
          <span class={['badge', `sev-${modeInfo.tone === 'ok' ? 'info' : modeInfo.tone}`]}>
            Продукційна ІСППР (Forward Chaining)
          </span>
          <span class="mono muted">{modeInfo.code}</span>
          <Tooltip
            title="Прямий логічний вивід"
            text="Машина виводу циклічно перевіряє правила ЯКЩО→ТО, додає виведені факти в робочу пам’ять і повторює прохід, доки не досягне нерухомої точки."
            align="start"
          />
        </div>
        <h1 class="hero-title">{modeInfo.title}</h1>
        <p class="hero-summary">{modeInfo.summary}</p>
      </div>

      <div class="stat-grid">
        <div class="stat-box">
          <span class="k-row">
            <span class="k">Активні рішення</span>
            <Tooltip
              text="Кількість рекомендацій, що сформовані на основі поточної робочої пам’яті."
              align="end"
            />
          </span>
          <span class="v num">{snapshot.inference.decisions.length}</span>
        </div>
        <div class="stat-box">
          <span class="k-row">
            <span class="k">Спрацювало правил</span>
            <Tooltip
              text="Частка правил із файлу knowledge/rules.toml, усі умови яких виконано."
              align="end"
            />
          </span>
          <span class="v num">{snapshot.inference.fired_rule_ids.length} / {rules.length}</span>
        </div>
        <div class="stat-box">
          <span class="k-row">
            <span class="k">Ітерацій виводу</span>
            <Tooltip
              title="Багатокрокове ланцюжкування"
              text="Кількість проходів машини Forward Chaining: виведені факти з першого кроку запускають узагальнюючі правила на наступних кроках."
              align="end"
            />
          </span>
          <span class="v num">{snapshot.inference.iterations}</span>
        </div>
        <div class="stat-box">
          <span class="k-row">
            <span class="k">Фактів у пам’яті</span>
            <Tooltip
              text="Загальна кількість фактів: зібраних Web-агентом, введених вручну та виведених правилами."
              align="end"
            />
          </span>
          <span class="v num">{allFacts.length}</span>
        </div>
      </div>
    </div>
  </section>

  <!-- 2. Decisions & Proof Traces -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          Сформовані рішення та ланцюжки доведення
          <Tooltip
            title="Модель впевненості MYCIN"
            text="CF висновку = min(CF усіх умов) × CF правила. Якщо два правила виводять однаковий факт, застосовується комбінація CF₁ + CF₂×(1 − CF₁)."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Кожне рішення супроводжується коефіцієнтом впевненості (CF) і повним поясненням умов, які
          призвели до спрацювання правила.
        </p>
      </div>

      <div class="filter-pills" role="group" aria-label="Фільтр за важливістю">
        {#each ['all', 'critical', 'high', 'medium', 'info'] as const as sev (sev)}
          <button
            type="button"
            class={['pill-btn', severityFilter === sev && 'active']}
            onclick={() => (severityFilter = sev)}
          >
            {sev === 'all' ? 'Усі' : severityLabelUk(sev)}
          </button>
        {/each}
      </div>
    </div>

    {#if filteredDecisions.length === 0}
      <div class="empty-state">
        За обраним рівнем важливості активних рішень немає. Спробуйте скористатися симулятором нижче.
      </div>
    {:else}
      <div class="decisions-list">
        {#each filteredDecisions as dec, decIdx (dec.rule_id)}
          {@render decisionCard(dec, decIdx < 3)}
        {/each}
      </div>
    {/if}
  </section>

  <!-- 3. Interactive What-If Simulator -->
  <section class="panel simulator-panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          Симулятор «Що буде, якщо…» (ручні факти)
          <Tooltip
            title="Моделювання сценаріїв What-If"
            text="Ручні факти мають пріоритет над автоматичними фактами Web-агента, але не змінюють зовнішні дані. Їх можна скинути в один клік."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Перевизначте будь-який факт робочої пам’яті з власним коефіцієнтом впевненості (CF) —
          машина виводу миттєво перерахує операційний режим і рекомендації.
        </p>
      </div>

      {#if manualFactsList.length > 0}
        <button
          type="button"
          class="btn btn-danger btn-sm"
          disabled={clearingFacts}
          onclick={handleClearFacts}
        >
          {clearingFacts ? 'Очищення…' : `Скинути ручні факти (${manualFactsList.length})`}
        </button>
      {/if}
    </div>

    <div class="preset-bar">
      <span class="preset-label muted">Швидкі сценарії:</span>
      {#each PRESETS as preset (preset.label)}
        <button
          type="button"
          class="preset-btn"
          onclick={() => applyPreset(preset)}
          title={preset.hint}
        >
          {preset.label}
        </button>
      {/each}
    </div>

    {#if formError}
      <div class="form-error" role="alert">{formError}</div>
    {/if}

    <form class="sim-form" onsubmit={handleUpsertFact}>
      <label class="field">
        <span class="field-label">Ключ факту</span>
        <input
          type="text"
          name="key"
          list="dss-fact-keys"
          required
          placeholder="наприклад, cve_mentions_count"
          bind:value={factKey}
        />
        <datalist id="dss-fact-keys">
          {#each suggestedKeys as k (k)}
            <option value={k}></option>
          {/each}
        </datalist>
      </label>

      <label class="field">
        <span class="field-label">Значення (число, true/false або рядок)</span>
        <input
          type="text"
          name="value"
          required
          placeholder="наприклад, 3 або high"
          bind:value={factValue}
        />
      </label>

      <label class="field cf-field">
        <span class="field-label">
          Коефіцієнт впевненості (CF): <strong class="mono num">{factCf.toFixed(2)}</strong>
        </span>
        <input type="range" name="cf" min="0" max="1" step="0.05" bind:value={factCf} />
      </label>

      <div class="field-actions">
        <button type="submit" class="btn" disabled={submittingFact}>
          {submittingFact ? 'Перерахунок…' : 'Застосувати факт і перерахувати'}
        </button>
      </div>
    </form>

    {#if manualFactsList.length > 0}
      <div class="manual-active">
        <span class="muted">Активні ручні перевизначення:</span>
        {#each manualFactsList as mf (mf.key)}
          <span class="badge badge-kw mono">
            {mf.key} = {formatFactValue(mf.value)} (CF {mf.cf.toFixed(2)})
          </span>
        {/each}
      </div>
    {/if}
  </section>

  <!-- 4. Working Memory Facts Explorer -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          Робоча пам’ять ІСППР (факти)
          <Tooltip
            title="Джерела фактів"
            text="«Web-агент» — зібрано автоматично з HTTP/RSS/НБУ; «Ручне введення» — задано у симуляторі; «Виведено ІСППР» — похідні факти від спрацьованих правил."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Об’єднаний набір вхідних фактів від Web-агента, ручних симуляцій і похідних фактів,
          виведених правилами.
        </p>
      </div>
      <span class="muted num">Показано {filteredFacts.length} із {allFacts.length}</span>
    </div>

    <div class="facts-controls">
      <input
        type="search"
        class="search-input"
        placeholder="Пошук факту за ключем, описом або значенням…"
        bind:value={factSearch}
        aria-label="Пошук фактів"
      />

      <div class="filter-pills" role="group" aria-label="Фільтр за походженням факту">
        {#each ['all', 'web_agent', 'inferred', 'manual'] as const as orig (orig)}
          <button
            type="button"
            class={['pill-btn', originFilter === orig && 'active']}
            onclick={() => (originFilter = orig)}
          >
            {orig === 'all' ? 'Усі джерела' : originLabelUk(orig)}
          </button>
        {/each}
      </div>
    </div>

    {#if filteredFacts.length === 0}
      <div class="empty-state">Фактів за обраними критеріями не знайдено.</div>
    {:else}
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Факт і пояснення</th>
              <th>Значення</th>
              <th>Походження</th>
              <th>Впевненість (CF)</th>
              <th>Оновлено</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredFacts as fact (fact.key)}
              <tr>
                <td>
                  <div class="mono"><strong>{fact.key}</strong></div>
                  <div class="muted">{factDescriptionUk(fact.key)}</div>
                </td>
                <td class="mono num">{formatFactValue(fact.value)}</td>
                <td>
                  <span
                    class={[
                      'badge',
                      fact.origin === 'manual'
                        ? 'sev-high'
                        : fact.origin === 'inferred'
                          ? 'badge-kw'
                          : 'badge-group',
                    ]}
                  >
                    {originLabelUk(fact.origin)}
                  </span>
                  <div class="muted">{fact.source}</div>
                </td>
                <td>
                  <CfMeter value={fact.cf} label={fact.cf.toFixed(2)} width="62px" />
                </td>
                <td class="muted">{formatDateTimeUk(fact.updated_at)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>

  <!-- 5. Production Rules Catalog -->
  <section class="panel">
    <div class="panel-header">
      <div>
        <h2 class="panel-title title-with-tip">
          База продукційних правил ({rules.length})
          <Tooltip
            title="Декларативна база знань"
            text="Усі правила завантажуються з knowledge/rules.toml. Зелена рамка означає, що правило активне на поточних фактах."
            align="start"
          />
        </h2>
        <p class="panel-desc">
          Повний перелік правил формату ЯКЩО (умови) → ТО (похідний факт і рекомендація) з бази знань
          TOML.
        </p>
      </div>

      <div class="filter-pills" role="group" aria-label="Фільтр правил">
        <button
          type="button"
          class={['pill-btn', ruleFilter === 'all' && 'active']}
          onclick={() => (ruleFilter = 'all')}
        >
          Усі ({rules.length})
        </button>
        <button
          type="button"
          class={['pill-btn', ruleFilter === 'fired' && 'active']}
          onclick={() => (ruleFilter = 'fired')}
        >
          Спрацювали ({snapshot.inference.fired_rule_ids.length})
        </button>
        <button
          type="button"
          class={['pill-btn', ruleFilter === 'idle' && 'active']}
          onclick={() => (ruleFilter = 'idle')}
        >
          Неактивні ({Math.max(0, rules.length - snapshot.inference.fired_rule_ids.length)})
        </button>
      </div>
    </div>

    <div class="rules-grid">
      {#each filteredRules as rule (rule.id)}
        {@render ruleCard(rule, firedSet.has(rule.id))}
      {/each}
    </div>
  </section>
{/snippet}

<svelte:boundary>
  {@const uiState = await store.load()}
  {@render dssContent(uiState.snapshot, uiState.rules)}

  {#snippet pending()}
    <LoadingSkeleton variant="dss" message="Завантаження рішень і бази знань ІСППР…" />
  {/snippet}

  {#snippet failed(err, reset)}
    <ErrorBanner
      error={err}
      title="Не вдалося завантажити дані ІСППР"
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

  .dss-hero {
    border-left: 5px solid var(--accent);
  }

  .dss-hero.tone-critical {
    border-left-color: var(--critical-solid);
  }

  .dss-hero.tone-high {
    border-left-color: var(--high-solid);
  }

  .dss-hero.tone-medium {
    border-left-color: var(--medium-solid);
  }

  .dss-hero.tone-ok {
    border-left-color: var(--ok-solid);
  }

  .hero-row {
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1.15fr);
    gap: 1.25rem;
    align-items: center;
  }

  .eyebrow {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.4rem;
  }

  .hero-title {
    margin: 0;
    font-size: 1.45rem;
    line-height: 1.25;
  }

  .hero-summary {
    margin: 0.45rem 0 0;
    color: var(--text-secondary);
    font-size: 0.93rem;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
  }

  .stat-box {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 0.7rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .k-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.35rem;
  }

  .stat-box .k {
    font-size: 0.74rem;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--muted);
  }

  .stat-box .v {
    font-size: 1.32rem;
    font-weight: 800;
  }

  .filter-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .pill-btn {
    background: var(--surface-subtle);
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.32rem 0.75rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
  }

  .pill-btn:hover {
    background: #e2e8f0;
  }

  .pill-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #ffffff;
  }

  .decisions-list {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  .decision-card {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-left: 4px solid var(--info-solid);
    border-radius: var(--radius-md);
    padding: 1rem 1.15rem;
  }

  .decision-card.sev-border-critical {
    border-left-color: var(--critical-solid);
  }

  .decision-card.sev-border-high {
    border-left-color: var(--high-solid);
  }

  .decision-card.sev-border-medium {
    border-left-color: var(--medium-solid);
  }

  .dec-top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.45rem;
  }

  .dec-badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
  }

  .dec-title {
    margin: 0 0 0.3rem;
    font-size: 1.04rem;
  }

  .dec-rec {
    margin: 0 0 0.75rem;
    color: var(--text-secondary);
    font-size: 0.91rem;
  }

  .trace-wrap {
    margin-top: 0.6rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }

  .simulator-panel {
    background: linear-gradient(180deg, #f8fbff 0%, #ffffff 100%);
    border-color: var(--accent-border);
  }

  .preset-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.9rem;
  }

  .preset-label {
    font-weight: 600;
  }

  .preset-btn {
    background: var(--surface);
    color: var(--accent);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-sm);
    padding: 0.3rem 0.68rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
  }

  .preset-btn:hover {
    background: var(--accent-soft);
  }

  .form-error {
    margin-bottom: 0.85rem;
    padding: 0.6rem 0.85rem;
    border-radius: var(--radius-sm);
    background: var(--critical-bg);
    color: var(--critical-fg);
    border: 1px solid var(--critical-border);
    font-size: 0.86rem;
    font-weight: 600;
  }

  .sim-form {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)) auto;
    gap: 0.85rem;
    align-items: end;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .field-label {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .field input[type='text'] {
    padding: 0.48rem 0.72rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-size: 0.88rem;
    font-family: inherit;
    background: var(--surface);
    color: var(--text);
  }

  .cf-field input[type='range'] {
    width: 100%;
    margin: 0.45rem 0;
    accent-color: var(--accent);
  }

  .field-actions {
    display: flex;
    align-items: flex-end;
  }

  .manual-active {
    margin-top: 0.9rem;
    padding-top: 0.75rem;
    border-top: 1px dashed var(--border-strong);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
  }

  .facts-controls {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.9rem;
  }

  .search-input {
    flex: 1 1 260px;
    padding: 0.48rem 0.8rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-size: 0.88rem;
    font-family: inherit;
    background: var(--surface);
    color: var(--text);
  }

  .rules-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.85rem;
  }

  .rule-card {
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 0.95rem 1.05rem;
  }

  .rule-card.rule-fired {
    border-color: var(--ok-border);
    background: #f9fefb;
  }

  .rule-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.4rem;
  }

  .rule-badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }

  .rule-name {
    margin: 0 0 0.2rem;
    font-size: 0.97rem;
  }

  .rule-desc {
    margin: 0 0 0.65rem;
    font-size: 0.84rem;
  }

  .rule-logic {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding-top: 0.55rem;
    border-top: 1px dashed var(--border);
    font-size: 0.83rem;
  }

  .logic-block {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
  }

  .logic-kw {
    font-size: 0.7rem;
    font-weight: 800;
    letter-spacing: 0.04em;
    padding: 0.16rem 0.42rem;
    border-radius: 4px;
    background: #e2e8f0;
    color: var(--text);
    flex-shrink: 0;
    margin-top: 0.1rem;
  }

  .cond-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .cond-pill {
    padding: 0.14rem 0.46rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.78rem;
  }

  .then-col {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    align-items: flex-start;
  }

  .rule-rec {
    color: var(--text-secondary);
  }

  @media (max-width: 960px) {
    .hero-row,
    .sim-form,
    .rules-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
