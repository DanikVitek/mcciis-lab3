import * as z from 'zod/mini';

export const FactValueSchema = z.union([z.boolean(), z.number(), z.string()]);
export type FactValue = z.infer<typeof FactValueSchema>;

export const FactOriginSchema = z.enum(['web_agent', 'inferred', 'manual']);
export type FactOrigin = z.infer<typeof FactOriginSchema>;

export const OperatorSchema = z.enum(['eq', 'ne', 'gt', 'gte', 'lt', 'lte', 'contains']);
export type Operator = z.infer<typeof OperatorSchema>;

export const SeveritySchema = z.enum(['info', 'medium', 'high', 'critical']);
export type Severity = z.infer<typeof SeveritySchema>;

export const ErrorKindSchema = z.enum(['parse', 'transient']);
export type ErrorKind = z.infer<typeof ErrorKindSchema>;

export const LogRotationSchema = z.enum(['hourly', 'daily', 'never']);
export type LogRotation = z.infer<typeof LogRotationSchema>;

export const OperationalModeSchema = z.enum([
  'crisis_response',
  'enhanced_monitoring',
  'financial_watch',
  'routine',
]);
export type OperationalMode = z.infer<typeof OperationalModeSchema>;

export function parseOperationalMode(value: unknown): OperationalMode | null {
  const parsed = OperationalModeSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}

export type Tone = 'critical' | 'high' | 'medium' | 'ok' | 'info';

export const SourceHealthSchema = z.object({
  disabled: z.boolean(),
  consecutive_failures: z.number(),
  last_error: z.nullable(z.string()),
  last_error_kind: z.nullable(ErrorKindSchema),
  next_retry_at: z.nullable(z.string()),
  last_checked: z.nullable(z.string()),
});
export type SourceHealth = z.infer<typeof SourceHealthSchema>;

export const HttpSiteStateSchema = z.object({
  id: z.string(),
  name: z.string(),
  url: z.string(),
  category: z.string(),
  status_code: z.nullable(z.number()),
  content_length: z.nullable(z.number()),
  body_size: z.nullable(z.number()),
  previous_size: z.nullable(z.number()),
  content_hash: z.nullable(z.string()),
  etag: z.nullable(z.string()),
  last_modified: z.nullable(z.string()),
  changed: z.boolean(),
  acknowledged: z.boolean(),
  change_count: z.number(),
  health: SourceHealthSchema,
  last_changed: z.nullable(z.string()),
});
export type HttpSiteState = z.infer<typeof HttpSiteStateSchema>;

export const FeedMonitorStateSchema = z.object({
  id: z.string(),
  name: z.string(),
  group: z.string(),
  url: z.string(),
  matched_count: z.number(),
  health: SourceHealthSchema,
});
export type FeedMonitorState = z.infer<typeof FeedMonitorStateSchema>;

export const MatchedKeywordSchema = z.object({
  word: z.string(),
  weight: z.number(),
  in_title: z.boolean(),
  in_body: z.boolean(),
  contribution: z.number(),
});
export type MatchedKeyword = z.infer<typeof MatchedKeywordSchema>;

export const NewsArticleSchema = z.object({
  id: z.string(),
  feed_id: z.string(),
  feed_name: z.string(),
  group: z.string(),
  title: z.string(),
  link: z.string(),
  published: z.nullable(z.string()),
  summary: z.string(),
  matched_keywords: z.array(MatchedKeywordSchema),
  relevance_score: z.number(),
  cves: z.array(z.string()),
  fetched_at: z.string(),
});
export type NewsArticle = z.infer<typeof NewsArticleSchema>;

export const CurrencyRateSchema = z.object({
  cc: z.string(),
  txt: z.string(),
  rate: z.number(),
  previous_rate: z.nullable(z.number()),
  delta_pct: z.nullable(z.number()),
  exchangedate: z.string(),
});
export type CurrencyRate = z.infer<typeof CurrencyRateSchema>;

export const FactSchema = z.object({
  key: z.string(),
  value: FactValueSchema,
  source: z.string(),
  cf: z.number(),
  origin: FactOriginSchema,
  updated_at: z.string(),
});
export type Fact = z.infer<typeof FactSchema>;

export const ConditionSchema = z.object({
  fact: z.string(),
  op: OperatorSchema,
  value: FactValueSchema,
});
export type Condition = z.infer<typeof ConditionSchema>;

export const DerivedFactSpecSchema = z.object({
  key: z.string(),
  value: FactValueSchema,
});
export type DerivedFactSpec = z.infer<typeof DerivedFactSpecSchema>;

export const RuleSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  priority: z.number(),
  severity: SeveritySchema,
  cf: z.number(),
  conditions: z.array(ConditionSchema),
  derived_fact: z.nullable(DerivedFactSpecSchema),
  recommendation: z.string(),
});
export type Rule = z.infer<typeof RuleSchema>;

export const ConditionTraceSchema = z.object({
  fact_key: z.string(),
  operator: OperatorSchema,
  expected: FactValueSchema,
  actual: FactValueSchema,
  fact_source: z.string(),
  fact_cf: z.number(),
});
export type ConditionTrace = z.infer<typeof ConditionTraceSchema>;

export const DecisionSchema = z.object({
  rule_id: z.string(),
  rule_name: z.string(),
  priority: z.number(),
  severity: SeveritySchema,
  cf: z.number(),
  recommendation: z.string(),
  derived_fact: z.nullable(DerivedFactSpecSchema),
  explanation: z.array(ConditionTraceSchema),
});
export type Decision = z.infer<typeof DecisionSchema>;

export const InferenceResultSchema = z.object({
  decisions: z.array(DecisionSchema),
  inferred_facts: z.record(z.string(), FactSchema),
  fired_rule_ids: z.array(z.string()),
  iterations: z.number(),
});
export type InferenceResult = z.infer<typeof InferenceResultSchema>;

export const AgentLogEntrySchema = z.object({
  timestamp: z.string(),
  message: z.string(),
});
export type AgentLogEntry = z.infer<typeof AgentLogEntrySchema>;

export const AgentSnapshotSchema = z.object({
  sites: z.array(HttpSiteStateSchema),
  feeds: z.array(FeedMonitorStateSchema),
  articles: z.array(NewsArticleSchema),
  currencies: z.array(CurrencyRateSchema),
  nbu_health: SourceHealthSchema,
  agent_facts: z.record(z.string(), FactSchema),
  manual_facts: z.record(z.string(), FactSchema),
  inference: InferenceResultSchema,
  last_poll_at: z.nullable(z.string()),
  poll_count: z.number(),
  last_poll_duration_ms: z.nullable(z.number()),
  poll_in_progress: z.boolean(),
  recent_logs: z.array(AgentLogEntrySchema),
});
export type AgentSnapshot = z.infer<typeof AgentSnapshotSchema>;

export const AgentUpdateKindSchema = z.enum([
  'connected',
  'poll_started',
  'poll_completed',
  'state_changed',
]);
export type AgentUpdateKind = z.infer<typeof AgentUpdateKindSchema>;

export const AgentUpdateEventSchema = z.object({
  kind: AgentUpdateKindSchema,
  poll_count: z.number(),
  poll_in_progress: z.boolean(),
  changed_count: z.number(),
  articles_count: z.number(),
  decisions_count: z.number(),
  last_poll_duration_ms: z.nullable(z.number()),
  timestamp: z.string(),
});
export type AgentUpdateEvent = z.infer<typeof AgentUpdateEventSchema>;

export const RetryPolicySchema = z.object({
  parse_max_retries: z.number(),
  initial_backoff_secs: z.number(),
  max_backoff_secs: z.number(),
});

export const ServerConfigSchema = z.object({
  bind_addr: z.string(),
  poll_interval_secs: z.number(),
  request_timeout_secs: z.number(),
  max_concurrent_requests: z.number(),
  retry_policy: RetryPolicySchema,
});

export const LoggingConfigSchema = z.object({
  dir: z.string(),
  file_prefix: z.string(),
  rotation: LogRotationSchema,
  max_files: z.number(),
});

export const DssConfigSchema = z.object({
  rules_path: z.string(),
  state_path: z.string(),
});

export const NbuMetricsConfigSchema = z.object({
  enabled: z.boolean(),
  url: z.string(),
  currencies: z.array(z.string()),
});

export const HttpSiteConfigSchema = z.object({
  id: z.string(),
  name: z.string(),
  url: z.string(),
  category: z.string(),
});

export const KeywordCriterionSchema = z.object({
  word: z.string(),
  weight: z.number(),
});

export const NewsFeedConfigSchema = z.object({
  id: z.string(),
  name: z.string(),
  group: z.string(),
  url: z.string(),
  keywords: z.array(KeywordCriterionSchema),
});

export const AppConfigSchema = z.object({
  server: ServerConfigSchema,
  logging: LoggingConfigSchema,
  dss: DssConfigSchema,
  nbu_metrics: NbuMetricsConfigSchema,
  http_sites: z.array(HttpSiteConfigSchema),
  news_feeds: z.array(NewsFeedConfigSchema),
});
export type AppConfig = z.infer<typeof AppConfigSchema>;

export const UiStatePayloadSchema = z.object({
  snapshot: AgentSnapshotSchema,
  rules: z.array(RuleSchema),
  config: AppConfigSchema,
  pretty_toml: z.string(),
});
export type UiStatePayload = z.infer<typeof UiStatePayloadSchema>;

// Client-side form validation schema (What-If manual fact simulator)
export const ManualFactFormSchema = z.object({
  key: z.string().check(z.minLength(1, 'Ключ факту не може бути порожнім')),
  value: z.string().check(z.minLength(1, 'Значення факту не може бути порожнім')),
  cf: z.number().check(z.gte(0), z.lte(1)),
});
