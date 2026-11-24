<script lang="ts">
  import { resolve } from '$app/paths';
  import CfMeter from '#lib/components/CfMeter.svelte';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import { getUiState } from '#lib/ui-state.svelte.ts';

  const store = getUiState();

  const QUICK_STEPS = [
    {
      num: 1,
      title: 'Перевірте індикатор Live та операційний режим',
      desc: 'Зелений бейдж «Live» у верхній панелі означає активне WebSocket-з’єднання (/api/ws). Інтерфейс оновлюється автоматично після кожної ітерації Web-агента — сторінку перезавантажувати не потрібно.',
      linkLabel: 'Перейти до огляду →',
      route: '/' as const,
    },
    {
      num: 2,
      title: 'Контролюйте зміни HTTP/HTTPS-сайтів',
      desc: 'На головній сторінці відображаються ресурси, у яких змінився SHA-256 геш, ETag або розмір тіла. Натискайте «✓ Переглянуто», щоб підтвердити ознайомлення і зняти тривожний факт у ІСППР.',
      linkLabel: 'Відкрити моніторинг →',
      route: '/' as const,
    },
    {
      num: 3,
      title: 'Аналізуйте ранжовані новини та CVE',
      desc: 'Фільтруйте статті за тематикою, пошуковим запитом або чекбоксом «Лише з CVE». Клік на заголовок відкриває повний розбір формули релевантності та прямі посилання на базу NVD.',
      linkLabel: 'Переглянути стрічку →',
      route: '/' as const,
    },
    {
      num: 4,
      title: 'Досліджуйте вивід ІСППР і симулятор «Що буде, якщо…»',
      desc: 'У розділі «Рішення ІСППР» переглядайте ланцюжки доведення правил, робочу пам’ять фактів і тестуйте гіпотетичні сценарії (сплеск CVE, ріст курсу USD тощо) за допомогою швидких пресетів.',
      linkLabel: 'Відкрити ІСППР →',
      route: '/dss' as const,
    },
  ];

  const MODES = [
    {
      code: 'routine',
      title: 'Штатний режим моніторингу',
      badgeCls: 'badge-ok',
      borderCls: 'mode-ok',
      when: 'Немає критичних CVE-сигналів, валютних сплесків або непідтверджених змін сайтів.',
    },
    {
      code: 'financial_watch',
      title: 'Фінансовий моніторинг',
      badgeCls: 'sev-medium',
      borderCls: 'mode-medium',
      when: 'Курс USD/EUR за API НБУ або відносна зміна (delta_pct) перевищили порогові значення правил.',
    },
    {
      code: 'enhanced_monitoring',
      title: 'Посилений моніторинг',
      badgeCls: 'sev-high',
      borderCls: 'mode-high',
      when: 'Виявлено непереглянуті зміни на контрольованих HTTP-сайтах, високорелевантні кіберновини або збої джерел.',
    },
    {
      code: 'crisis_response',
      title: 'Режим кризового реагування',
      badgeCls: 'sev-critical',
      borderCls: 'mode-critical',
      when: 'Зафіксовано множинні згадки CVE або комбінацію критичних факторів кібербезпеки високої впевненості.',
    },
  ];

  const PIPELINE_STAGES = [
    {
      step: '01',
      title: 'Паралельний збір (Tokio)',
      body: 'Web-агент одночасно опитує HTTP/HTTPS-сайти, RSS/Atom-стрічки та JSON API НБУ з обмеженням паралелізму (Semaphore).',
    },
    {
      step: '02',
      title: 'Очищення, SHA-256 і ранжування',
      body: 'HTML-вміст хешується (SHA-256), новини очищуються від розмітки, оцінюються за зваженими ключовими словами й скануються на CVE.',
    },
    {
      step: '03',
      title: 'База фактів і Forward Chaining',
      body: 'Зібрані метрики формують факти робочої пам’яті. Продукційна ІСППР виконує прямий вивід із коефіцієнтами MYCIN CF.',
    },
    {
      step: '04',
      title: 'Миттєвий Push (WebSocket)',
      body: 'Після завершення ітерації Rust-сервер надсилає подію poll_completed у /api/ws, і браузер одразу оновлює всі віджети.',
    },
  ];
</script>

<svelte:head>
  <title>Інструкція користувача — Інтелектуальний Web-агент та ІСППР</title>
</svelte:head>

{#snippet quickStepCard(step: (typeof QUICK_STEPS)[number])}
  <article class="step-card">
    <div class="step-head">
      <span class="step-num num">{step.num}</span>
      <h3 class="step-title">{step.title}</h3>
    </div>
    <p class="step-desc">{step.desc}</p>
    <a class="step-link" href={resolve(step.route)}>{step.linkLabel}</a>
  </article>
{/snippet}

<!-- 1. Hero Welcome -->
<section class="panel guide-hero">
  <div class="hero-top">
    <div>
      <div class="eyebrow">
        <span class="badge sev-info">Посібник для нового користувача</span>
        <span class="badge badge-ok">
          WebSocket: {store.wsStatus === 'connected' ? 'підключено' : 'синхронізація'}
        </span>
      </div>
      <h1 class="hero-title">Як працювати з Web-агентом та ІСППР</h1>
      <p class="hero-summary">
        Ця система поєднує автономного Web-агента на Rust (моніторинг HTTP/HTTPS-сайтів, ранжування
        RSS/Atom-новин і курсів валют НБУ) із продукційною інтелектуальною системою підтримки
        прийняття рішень (ІСППР). На кожному віджеті інтерфейсу доступні контекстні підказки
        <Tooltip
          title="Приклад контекстної підказки"
          text="Наведіть курсор або перейдіть клавішею Tab на будь-яку іконку «?» у заголовках віджетів, щоб побачити коротке пояснення метрики чи формули."
          align="center"
        />
        — наведіть на них курсор для швидкої довідки.
      </p>
    </div>

    <div class="hero-cta">
      <a class="btn" href={resolve('/')}>Перейти до моніторингу</a>
      <a class="btn btn-secondary" href={resolve('/dss')}>Спробувати симулятор ІСППР</a>
      <a class="btn btn-secondary" href={resolve('/conf')}>Переглянути конфігурацію</a>
    </div>
  </div>

  <nav class="toc-pills" aria-label="Розділи інструкції">
    <a class="toc-link" href="#quick-start">1. Швидкий старт</a>
    <a class="toc-link" href="#architecture">2. Цикл роботи та WebSocket</a>
    <a class="toc-link" href="#modes-and-formulas">3. Режими, формули й MYCIN CF</a>
    <a class="toc-link" href="#resilience">4. Відмовостійкість (Backoff / CB)</a>
    <a class="toc-link" href="#routes-api">5. Сторінки та API</a>
  </nav>
</section>

<!-- 2. Quick Start -->
<section id="quick-start" class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">1. Швидкий старт за 4 кроки</h2>
      <p class="panel-desc">
        Послідовність дій для швидкого ознайомлення з можливостями системи.
      </p>
    </div>
  </div>

  <div class="steps-grid">
    {#each QUICK_STEPS as step (step.num)}
      {@render quickStepCard(step)}
    {/each}
  </div>
</section>

<!-- 3. Architecture & Live WebSocket Updates -->
<section id="architecture" class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">2. Як дані потрапляють на екран у реальному часі</h2>
      <p class="panel-desc">
        Від зовнішніх веб-джерел до логічних висновків ІСППР і автоматичного оновлення інтерфейсу.
      </p>
    </div>
  </div>

  <div class="pipeline-grid">
    {#each PIPELINE_STAGES as stage (stage.step)}
      <div class="pipe-card">
        <div class="pipe-badge mono">{stage.step}</div>
        <h3 class="pipe-title">{stage.title}</h3>
        <p class="pipe-body">{stage.body}</p>
      </div>
    {/each}
  </div>

  <div class="callout-box">
    <strong>Оновлення без перезавантаження сторінки:</strong>
    браузер тримає відкритим WebSocket-канал <code>/api/ws</code>. Щойно агент розпочинає цикл
    опитування (<code>poll_started</code>), з’являється індикатор прогресу; по завершенні
    (<code>poll_completed</code>) інтерфейс автоматично завантажує свіжий стан і показує коротке
    сповіщення. Ви також можете запустити позачергову перевірку кнопкою
    <strong>«↻ Оновити зараз»</strong> у шапці.
  </div>
</section>

<!-- 4. Modes, Relevance Formula & MYCIN CF -->
<section id="modes-and-formulas" class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">3. Як читати показники, режими та формули</h2>
      <p class="panel-desc">
        Усі оцінки та рішення системи є прозорими й відтворюваними.
      </p>
    </div>
  </div>

  <h3 class="sub-heading">Операційні режими ІСППР</h3>
  <div class="modes-grid">
    {#each MODES as m (m.code)}
      <div class={['mode-card', m.borderCls]}>
        <div class="mode-top">
          <span class={['badge', m.badgeCls]}>{m.title}</span>
          <span class="mono muted">{m.code}</span>
        </div>
        <p class="mode-when">{m.when}</p>
      </div>
    {/each}
  </div>

  <div class="formula-split">
    <div class="info-card">
      <h3 class="info-title">Формула рейтингу релевантності новини</h3>
      <div class="formula-code mono">
        Score = Σ wₖ · (2·I_title + 1·I_body) + 0.75 · max(0, |M| − 1) + 2.0 · I_CVE
      </div>
      <ul class="bullet-list">
        <li>
          <strong>wₖ</strong> — вага ключового слова зі стрічки (налаштовується у
          <code>config.toml</code>).
        </li>
        <li>
          <strong>×2 у заголовку, ×1 у тексті</strong> — збіг у заголовку має подвійний пріоритет.
        </li>
        <li>
          <strong>Бонус різноманіття (+0.75)</strong> — заохочує статті, де знайдено кілька різних
          ключових слів <code>|M|</code>.
        </li>
        <li>
          <strong>Бонус CVE (+2.0)</strong> — додається автоматично при виявленні хоча б одного
          ідентифікатора <code>CVE-YYYY-NNNN</code>.
        </li>
      </ul>
    </div>

    <div class="info-card">
      <h3 class="info-title">Коефіцієнт впевненості MYCIN (CF)</h3>
      <div class="formula-code mono">
        CF_out = min(CF_фактів) × CF_правила<br />
        CF_comb = CF₁ + CF₂ · (1 − CF₁)
      </div>
      <div class="cf-demo-list">
        <div class="cf-demo-row">
          <span class="muted">Висока впевненість:</span>
          <CfMeter value={0.92} tone="critical" label="CF 0.92" width="84px" />
        </div>
        <div class="cf-demo-row">
          <span class="muted">Помірна впевненість:</span>
          <CfMeter value={0.68} tone="medium" label="CF 0.68" width="84px" />
        </div>
      </div>
      <ul class="bullet-list">
        <li>
          Шкала <strong>CF ∈ [0.00, 1.00]</strong> відображає надійність висновку на основі
          достовірності вхідних фактів.
        </li>
        <li>
          У <strong>симуляторі «Що буде, якщо…»</strong> на сторінці
          <a href={resolve('/dss')}>Рішення ІСППР</a> ви можете змінювати CF повзунком і одразу
          бачити, як змінюються висновки правил.
        </li>
      </ul>
    </div>
  </div>
</section>

<!-- 5. Fault Tolerance -->
<section id="resilience" class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">4. Відмовостійкість зовнішніх джерел (Backoff та Circuit Breaker)</h2>
      <p class="panel-desc">
        Проблеми одного зовнішнього сайту або RSS-стрічки ніколи не блокують роботу Web-агента.
      </p>
    </div>
  </div>

  <div class="resilience-grid">
    <div class="info-card">
      <div class="res-head">
        <span class="badge sev-high">Пауза (Exponential Backoff)</span>
      </div>
      <p class="info-p">
        При тимчасових мережевих помилках (таймаут, HTTP 5xx, втрата зв’язку) джерело отримує
        зростаючу паузу між спробами (наприклад, <code>30с → 60с → 120с → …</code>). Після
        відновлення доступу таймер автоматично скидається.
      </p>
    </div>

    <div class="info-card">
      <div class="res-head">
        <span class="badge sev-critical">Вимкнено (Circuit Breaker)</span>
      </div>
      <p class="info-p">
        Якщо зовнішнє джерело кілька разів поспіль повертає некоректний XML/RSS або JSON
        (<code>parse_max_retries</code>), захисний механізм вимикає лише це джерело та показує
        попередження на головній сторінці. Після виправлення його можна повернути в роботу однією
        кнопкою <strong>«Відновити зараз»</strong>.
      </p>
    </div>
  </div>
</section>

<!-- 6. Pages & API Reference -->
<section id="routes-api" class="panel">
  <div class="panel-header">
    <div>
      <h2 class="panel-title">5. Навігація по сторінках і маршрути API</h2>
      <p class="panel-desc">Швидка шпаргалка по інтерфейсу та інтеграційним точкам Rust-сервера.</p>
    </div>
  </div>

  <div class="table-wrap">
    <table>
      <thead>
        <tr>
          <th>Маршрут</th>
          <th>Тип</th>
          <th>Призначення</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td class="mono"><a href={resolve('/')}>/</a></td>
          <td><span class="badge badge-group">UI</span></td>
          <td>Головна панель: операційний режим, KPI, курси НБУ, зміни HTTP-сайтів, ранжовані новини та журнал.</td>
        </tr>
        <tr>
          <td class="mono">/article/:id</td>
          <td><span class="badge badge-group">UI</span></td>
          <td>Деталі відібраної статті: очищений текст, посилання на NVD для CVE та покрокова формула релевантності.</td>
        </tr>
        <tr>
          <td class="mono"><a href={resolve('/dss')}>/dss</a></td>
          <td><span class="badge badge-group">UI</span></td>
          <td>Рішення ІСППР із ланцюжками доведення, інтерактивний симулятор «Що буде, якщо…», робоча пам’ять і база правил.</td>
        </tr>
        <tr>
          <td class="mono"><a href={resolve('/conf')}>/conf</a></td>
          <td><span class="badge badge-group">UI</span></td>
          <td>Активна конфігурація сервера, сходинки backoff, списки сайтів, RSS-стрічки з вагами та канонічний TOML.</td>
        </tr>
        <tr>
          <td class="mono"><a href={resolve('/guide')}>/guide</a></td>
          <td><span class="badge badge-ok">UI</span></td>
          <td>Ця сторінка інструкції користувача.</td>
        </tr>
        <tr>
          <td class="mono">/api/ws</td>
          <td><span class="badge badge-kw">WebSocket</span></td>
          <td>Потік подій реального часу (<code>connected</code>, <code>poll_started</code>, <code>poll_completed</code>, <code>state_changed</code>).</td>
        </tr>
        <tr>
          <td class="mono"><a href="/api/state" target="_blank" rel="noopener">/api/state ↗</a></td>
          <td><span class="badge badge-group">JSON API</span></td>
          <td>Повний знімок стану Web-агента та результатів виводу ІСППР у форматі JSON.</td>
        </tr>
        <tr>
          <td class="mono">/api/ui-state</td>
          <td><span class="badge badge-group">JSON API</span></td>
          <td>Об’єднаний payload для інтерфейсу (знімок, правила бази знань, конфігурація, форматований TOML).</td>
        </tr>
      </tbody>
    </table>
  </div>
</section>

<style>
  .guide-hero {
    border-left: 5px solid var(--accent);
    background: linear-gradient(180deg, #f8fbff 0%, #ffffff 70%);
  }

  .hero-top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1.15rem;
  }

  .eyebrow {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.45rem;
  }

  .hero-title {
    margin: 0;
    font-size: 1.52rem;
    line-height: 1.25;
  }

  .hero-summary {
    margin: 0.5rem 0 0;
    max-width: 68ch;
    color: var(--text-secondary);
    font-size: 0.94rem;
  }

  .hero-cta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .toc-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    margin-top: 1.1rem;
    padding-top: 0.95rem;
    border-top: 1px solid var(--border);
  }

  .toc-link {
    display: inline-flex;
    align-items: center;
    padding: 0.28rem 0.72rem;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
    text-decoration: none;
    background: var(--surface-subtle);
    color: var(--text-secondary);
    border: 1px solid var(--border-strong);
  }

  .toc-link:hover {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent-border);
  }

  .steps-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.95rem;
  }

  .step-card {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    padding: 1rem 1.1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .step-head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .step-num {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 50%;
    background: var(--accent);
    color: #ffffff;
    font-size: 0.82rem;
    font-weight: 800;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .step-title {
    margin: 0;
    font-size: 0.98rem;
    line-height: 1.3;
  }

  .step-desc {
    margin: 0;
    font-size: 0.87rem;
    color: var(--text-secondary);
    flex: 1;
  }

  .step-link {
    font-size: 0.84rem;
    font-weight: 600;
    text-decoration: none;
  }

  .step-link:hover {
    text-decoration: underline;
  }

  .pipeline-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.85rem;
  }

  .pipe-card {
    padding: 0.95rem 1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .pipe-badge {
    display: inline-block;
    padding: 0.12rem 0.48rem;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    border: 1px solid var(--accent-border);
    font-weight: 800;
    font-size: 0.76rem;
    margin-bottom: 0.45rem;
  }

  .pipe-title {
    margin: 0 0 0.3rem;
    font-size: 0.94rem;
  }

  .pipe-body {
    margin: 0;
    font-size: 0.84rem;
    color: var(--text-secondary);
  }

  .callout-box {
    margin-top: 1rem;
    padding: 0.85rem 1rem;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    border: 1px solid var(--accent-border);
    color: var(--text-secondary);
    font-size: 0.87rem;
  }

  .callout-box strong {
    color: var(--text);
  }

  .sub-heading {
    margin: 0 0 0.7rem;
    font-size: 0.96rem;
    color: var(--text);
  }

  .modes-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.8rem;
    margin-bottom: 1.15rem;
  }

  .mode-card {
    padding: 0.85rem 1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: var(--radius-md);
  }

  .mode-ok {
    border-left-color: var(--ok-solid);
  }

  .mode-medium {
    border-left-color: var(--medium-solid);
  }

  .mode-high {
    border-left-color: var(--high-solid);
  }

  .mode-critical {
    border-left-color: var(--critical-solid);
  }

  .mode-top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
  }

  .mode-when {
    margin: 0;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .formula-split,
  .resilience-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.95rem;
  }

  .info-card {
    padding: 1rem 1.1rem;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .info-title {
    margin: 0 0 0.55rem;
    font-size: 0.96rem;
  }

  .formula-code {
    padding: 0.65rem 0.8rem;
    border-radius: var(--radius-sm);
    background: #0f172a;
    color: #e2e8f0;
    font-size: 0.8rem;
    line-height: 1.55;
    margin-bottom: 0.7rem;
    overflow-x: auto;
  }

  .cf-demo-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
    margin-bottom: 0.65rem;
  }

  .cf-demo-row {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.82rem;
  }

  .bullet-list {
    margin: 0;
    padding-left: 1.15rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .res-head {
    margin-bottom: 0.5rem;
  }

  .info-p {
    margin: 0;
    font-size: 0.87rem;
    color: var(--text-secondary);
  }

  @media (max-width: 960px) {
    .steps-grid,
    .pipeline-grid,
    .modes-grid,
    .formula-split,
    .resilience-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
