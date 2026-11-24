import {
  parseOperationalMode,
  type AgentSnapshot,
  type ErrorKind,
  type Fact,
  type FactOrigin,
  type FactValue,
  type HttpSiteState,
  type OperationalMode,
  type Operator,
  type Severity,
  type SourceHealth,
  type Tone,
} from './schemas';

export interface ModeDescription {
  code: OperationalMode | 'collecting';
  title: string;
  summary: string;
  tone: Tone;
}

function assertNever(x: never): never {
  throw new Error(`Unexpected value: ${String(x)}`);
}

export function extractOperationalMode(snapshot: AgentSnapshot): OperationalMode | null {
  const inferred = snapshot.inference.inferred_facts['operational_mode'];
  const parsedInferred = parseOperationalMode(inferred?.value);
  if (parsedInferred !== null) return parsedInferred;

  const manual = snapshot.manual_facts['operational_mode'];
  return parseOperationalMode(manual?.value);
}

export function describeOperationalMode(mode: OperationalMode | null): ModeDescription {
  if (mode === null) {
    return {
      code: 'collecting',
      title: 'Первинний аналіз середовища',
      summary:
        'Web-агент збирає дані із зовнішніх ресурсів та формує факти робочої пам’яті для логічного виводу ІСППР.',
      tone: 'info',
    };
  }

  switch (mode) {
    case 'crisis_response':
      return {
        code: mode,
        title: 'Кризове реагування',
        summary:
          'Одночасно зафіксовано критичні кіберзагрози (CVE) та підвищений валютний тиск. Рекомендовано першочергово перевірити виявлені вразливості та валютні ризики.',
        tone: 'critical',
      };
    case 'enhanced_monitoring':
      return {
        code: mode,
        title: 'Посилений моніторинг',
        summary:
          'Високий потік подій кібербезпеки збігся зі змінами на контрольованих Web-сайтах. Перевірте оновлені ресурси та останні бюлетені.',
        tone: 'high',
      };
    case 'financial_watch':
      return {
        code: mode,
        title: 'Фінансовий контроль',
        summary:
          'Підвищені курси валют НБУ супроводжуються активним макроекономічним новинним фоном. Звірте валютні показники та рекомендації ІСППР.',
        tone: 'medium',
      };
    case 'routine':
      return {
        code: mode,
        title: 'Штатний режим',
        summary:
          'Інформаційне середовище стабільне: критичних кіберінцидентів і непереглянутих змін на сайтах немає.',
        tone: 'ok',
      };
    default:
      return assertNever(mode);
  }
}

const GROUP_LABELS: Readonly<Record<string, string>> = {
  cybersecurity: 'Кібербезпека',
  'tech-security': 'Технології та безпека',
  'rust-ecosystem': 'Екосистема Rust',
  economics: 'Економіка',
  'economics-world': 'Світова економіка',
  'test-http': 'Тестовий HTTP',
  'tech-news': 'Технологічні новини',
  mock: 'Тестове джерело',
};

export function groupLabelUk(group: string): string {
  return GROUP_LABELS[group] ?? group;
}

const FACT_DESCRIPTIONS: Readonly<Record<string, string>> = {
  operational_mode: 'Інтегральний операційний режим системи (виведено мета-правилами ІСППР)',
  cyber_threat_level: 'Оцінка рівня кіберзагроз на основі новин та CVE',
  fx_pressure_level: 'Рівень валютного тиску за офіційними курсами НБУ',
  fx_volatility: 'Оцінка короткострокової волатильності курсів валют',
  macro_news_activity: 'Інтенсивність макроекономічних та енергетичних новин',
  web_environment_state: 'Стан змін на контрольованих HTTP/HTTPS-сайтах',
  priority_signal: 'Наявність новини з високим рейтингом релевантності',
  cve_mentions_count: 'Кількість унікальних ідентифікаторів вразливостей CVE',
  detected_cves: 'Перелік усіх унікальних ідентифікаторів CVE, знайдених агентом',
  cyber_critical_articles: 'Кількість критичних статей із кібербезпеки',
  cyber_articles_count: 'Загальна кількість відібраних статей у групі кібербезпеки',
  econ_articles_count: 'Кількість релевантних економічних повідомлень',
  total_matched_articles: 'Загальна кількість відібраних релевантних новин',
  top_article_score: 'Найвища оцінка релевантності серед відібраних новин',
  http_changed_sites_count: 'Кількість HTTP/HTTPS-сайтів із непереглянутими змінами',
  http_monitored_sites_count: 'Загальна кількість контрольованих HTTP/HTTPS-сайтів',
  http_error_sites_count: 'Кількість HTTP/HTTPS-сайтів із помилками опитування',
  http_unreachable_sites_count: 'Кількість недоступних або вимкнених HTTP-сайтів',
  disabled_sources_count: 'Кількість джерел, автоматично вимкнених circuit breaker',
  nbu_rate_usd: 'Офіційний курс НБУ: долар США (грн)',
  nbu_rate_eur: 'Офіційний курс НБУ: євро (грн)',
  nbu_rate_pln: 'Офіційний курс НБУ: польський злотий (грн)',
  nbu_rate_gbp: 'Офіційний курс НБУ: британський фунт (грн)',
  nbu_eur_usd_ratio: 'Співвідношення курсів EUR / USD',
  nbu_max_abs_delta_pct: 'Максимальна абсолютна зміна курсу НБУ між циклами (%)',
};

export function factDescriptionUk(key: string): string {
  if (FACT_DESCRIPTIONS[key]) {
    return FACT_DESCRIPTIONS[key];
  }
  if (key.startsWith('site_') && key.endsWith('_changed')) {
    const id = key.slice('site_'.length, -'_changed'.length);
    return `Прапорець непереглянутої зміни HTTP-сайту «${id}»`;
  }
  if (key.startsWith('site_') && key.endsWith('_size_bytes')) {
    const id = key.slice('site_'.length, -'_size_bytes'.length);
    return `Розмір тіла відповіді HTTP-сайту «${id}» (байт)`;
  }
  if (key.startsWith('http_site_changed_')) {
    return `Прапорець непереглянутої зміни сайту «${key.replace('http_site_changed_', '')}»`;
  }
  if (key.startsWith('nbu_rate_')) {
    return `Офіційний курс валюти ${key.replace('nbu_rate_', '').toUpperCase()} (НБУ)`;
  }
  return 'Факт робочої пам’яті бази знань';
}

export function formatFactValue(value: FactValue): string {
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value === 'number') {
    if (Math.abs(value - Math.round(value)) < 1e-9) {
      return String(Math.round(value));
    }
    return value.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
  }
  return value;
}

export function operatorSymbol(op: Operator): string {
  switch (op) {
    case 'eq':
      return '=';
    case 'ne':
      return '≠';
    case 'gt':
      return '>';
    case 'gte':
      return '≥';
    case 'lt':
      return '<';
    case 'lte':
      return '≤';
    case 'contains':
      return 'містить';
    default:
      return assertNever(op);
  }
}

export function severityLabelUk(severity: Severity): string {
  switch (severity) {
    case 'critical':
      return 'Критичний';
    case 'high':
      return 'Високий';
    case 'medium':
      return 'Середній';
    case 'info':
      return 'Інформаційний';
    default:
      return assertNever(severity);
  }
}

export function originLabelUk(origin: FactOrigin): string {
  switch (origin) {
    case 'web_agent':
      return 'Web-агент';
    case 'inferred':
      return 'Виведено ІСППР';
    case 'manual':
      return 'Вручну';
    default:
      return assertNever(origin);
  }
}

export function errorKindLabelUk(kind: ErrorKind | null): string {
  if (kind === null) return 'Помилка джерела';
  switch (kind) {
    case 'parse':
      return 'Помилка парсингу';
    case 'transient':
      return 'Тимчасова мережева помилка';
    default:
      return assertNever(kind);
  }
}

export function formatDateTimeUk(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`;
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null || bytes === undefined) return '—';
  if (bytes < 1024) return `${bytes} Б`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ (${bytes.toLocaleString('uk-UA')} Б)`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} МБ`;
}

export function formatSizeDelta(
  curr: number | null,
  prev: number | null,
): { text: string; kind: 'up' | 'down' | 'new' | 'same' } {
  if (curr === null || prev === null) {
    return { text: 'перша фіксація', kind: 'new' };
  }
  const diff = curr - prev;
  if (diff > 0) return { text: `+${diff.toLocaleString('uk-UA')} Б`, kind: 'up' };
  if (diff < 0) return { text: `${diff.toLocaleString('uk-UA')} Б`, kind: 'down' };
  return { text: '0 Б (змінено вміст)', kind: 'same' };
}

export function isSiteChangedVisible(site: HttpSiteState): boolean {
  return !site.health.disabled && site.changed && !site.acknowledged;
}

export function isHealthThrottled(health: SourceHealth, nowMs = Date.now()): boolean {
  if (health.disabled || !health.next_retry_at) return false;
  const retryMs = Date.parse(health.next_retry_at);
  return !Number.isNaN(retryMs) && retryMs > nowMs;
}

export function countDisabledSources(snapshot: AgentSnapshot): number {
  return (
    snapshot.sites.filter((s) => s.health.disabled).length +
    snapshot.feeds.filter((f) => f.health.disabled).length +
    (snapshot.nbu_health.disabled ? 1 : 0)
  );
}

export function mergeAllFacts(snapshot: AgentSnapshot): Fact[] {
  const map = new Map<string, Fact>();
  for (const [k, v] of Object.entries(snapshot.agent_facts)) {
    map.set(k, v);
  }
  for (const [k, v] of Object.entries(snapshot.manual_facts)) {
    map.set(k, v);
  }
  for (const [k, v] of Object.entries(snapshot.inference.inferred_facts)) {
    if (!map.has(k)) {
      map.set(k, v);
    }
  }
  return Array.from(map.values()).sort((a, b) => a.key.localeCompare(b.key));
}

export interface ParsedSummary {
  sections: Array<{ heading: string | null; paragraphs: string[] }>;
  references: Array<{ index: string; url: string }>;
}

export function parseSummaryBlocks(raw: string): ParsedSummary {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { sections: [], references: [] };
  }

  const references: Array<{ index: string; url: string }> = [];
  const refRegex = /\[(\d+)\]:\s*(https?:\/\/\S+)/g;
  let bodyText = trimmed;
  let match: RegExpExecArray | null;
  while ((match = refRegex.exec(trimmed)) !== null) {
    references.push({ index: match[1], url: match[2].replace(/[.,;)]+$/, '') });
  }
  if (references.length > 0) {
    bodyText = bodyText.replace(refRegex, '').trim();
  }

  const tokens = bodyText
    .replace(/\s+(#{2,4}\s+)/g, '\n$1')
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);

  const sections: Array<{ heading: string | null; paragraphs: string[] }> = [];
  let current: { heading: string | null; paragraphs: string[] } = { heading: null, paragraphs: [] };

  for (const line of tokens) {
    const boldHeading = line.match(/^#{2,4}\s+\*\*([^*]+?)\*\*(?:\s+(.*))?$/);
    const plainHeading = !boldHeading
      ? line.match(/^#{2,4}\s+([A-ZА-ЯІЇЄҐ][^:.!?]{0,42}?)(?::\s+|\s+(?=[A-ZА-ЯІЇЄҐ]))(.*)$/)
      : null;
    const standaloneHeading = !boldHeading && !plainHeading ? line.match(/^#{2,4}\s+(.+)$/) : null;

    if (boldHeading || plainHeading || standaloneHeading) {
      if (current.heading || current.paragraphs.length > 0) {
        sections.push(current);
      }
      const m = boldHeading ?? plainHeading ?? standaloneHeading!;
      const cleanHeading = m[1].replace(/\*\*/g, '').replace(/\s*:\s*$/, '').trim();
      const rest = (m[2] ?? '').trim();
      current = {
        heading: cleanHeading,
        paragraphs: rest ? splitBulletsOrParagraphs(rest) : [],
      };
    } else {
      current.paragraphs.push(...splitBulletsOrParagraphs(line));
    }
  }

  if (current.heading || current.paragraphs.length > 0) {
    sections.push(current);
  }

  return { sections, references };
}

function splitBulletsOrParagraphs(text: string): string[] {
  const cleaned = text.replace(/\*\*/g, '').trim();
  if (!cleaned) return [];
  if (cleaned.includes(' * ')) {
    return cleaned
      .split(/\s+\*\s+/)
      .map((s) => s.replace(/^\*\s+/, '').trim())
      .filter(Boolean);
  }
  return [cleaned];
}

export function shortPreview(raw: string, maxLen = 170): string {
  const plain = raw
    .replace(/\[(\d+)\]:\s*https?:\/\/\S+/g, '')
    .replace(/#{1,4}\s+/g, '')
    .replace(/\*\*/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (plain.length <= maxLen) return plain;
  return `${plain.slice(0, maxLen).trimEnd()}…`;
}
