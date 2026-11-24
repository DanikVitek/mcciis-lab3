import { error, isHttpError } from '@sveltejs/kit';
import {
  ManualFactFormSchema,
  NewsArticleSchema,
  UiStatePayloadSchema,
  type NewsArticle,
  type UiStatePayload,
} from '#lib/schemas.ts';

export function formatApiError(err: unknown, fallback = 'Не вдалося виконати запит до API.'): string {
  if (isHttpError(err)) {
    return `${err.body.message} (HTTP ${err.status})`;
  }
  if (err instanceof Error && err.message.trim().length > 0) {
    return err.message;
  }
  return fallback;
}

async function extractErrorMessage(response: Response, fallback: string): Promise<string> {
  const text = await response.text().catch(() => '');
  if (!text) return fallback;
  try {
    const parsed: unknown = JSON.parse(text);
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      'error' in parsed &&
      typeof parsed.error === 'string' &&
      parsed.error.trim().length > 0
    ) {
      return parsed.error;
    }
  } catch {
    // Plain text response body
  }
  return text;
}

async function fetchValidatedUiState(
  fetcher: typeof fetch,
  path: string,
  init?: RequestInit,
): Promise<UiStatePayload> {
  let response: Response;
  try {
    response = await fetcher(path, init);
  } catch {
    error(
      503,
      'Не вдалося підключитися до API-сервера Web-агента. Переконайтеся, що web-agent-server запущено.',
    );
  }

  if (!response.ok) {
    const detail = await extractErrorMessage(
      response,
      `API-сервер повернув помилку ${response.status}.`,
    );
    error(response.status, detail);
  }

  const raw: unknown = await response.json();
  const parsed = UiStatePayloadSchema.safeParse(raw);
  if (!parsed.success) {
    error(502, `Некоректний формат відповіді від API-сервера: ${parsed.error.message}`);
  }
  return parsed.data;
}

export function fetchUiState(fetcher: typeof fetch = fetch): Promise<UiStatePayload> {
  return fetchValidatedUiState(fetcher, '/api/ui-state');
}

export async function fetchArticleById(
  id: string,
  fetcher: typeof fetch = fetch,
): Promise<NewsArticle | null> {
  const url = `/api/articles/${encodeURIComponent(id)}`;
  let response: Response;
  try {
    response = await fetcher(url);
  } catch {
    error(503, 'Не вдалося підключитися до API-сервера Web-агента.');
  }

  if (response.status === 404) {
    return null;
  }

  if (!response.ok) {
    const detail = await extractErrorMessage(
      response,
      `Помилка завантаження новини (${response.status}).`,
    );
    error(response.status, detail);
  }

  const raw: unknown = await response.json();
  const parsed = NewsArticleSchema.safeParse(raw);
  if (!parsed.success) {
    error(502, `Некоректний формат новини від API-сервера: ${parsed.error.message}`);
  }
  return parsed.data;
}

function postAction(
  path: string,
  body?: Record<string, unknown>,
  fetcher: typeof fetch = fetch,
): Promise<UiStatePayload> {
  return fetchValidatedUiState(fetcher, path, {
    method: 'POST',
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
}

export function triggerPoll(): Promise<UiStatePayload> {
  return postAction('/api/poll');
}

export function ackSite(id: string): Promise<UiStatePayload> {
  return postAction(`/api/sites/${encodeURIComponent(id)}/ack`);
}

export function unackSite(id: string): Promise<UiStatePayload> {
  return postAction(`/api/sites/${encodeURIComponent(id)}/unack`);
}

export function enableSite(id: string): Promise<UiStatePayload> {
  return postAction(`/api/sites/${encodeURIComponent(id)}/enable`);
}

export function enableFeed(id: string): Promise<UiStatePayload> {
  return postAction(`/api/feeds/${encodeURIComponent(id)}/enable`);
}

export function enableNbu(): Promise<UiStatePayload> {
  return postAction('/api/nbu/enable');
}

export function upsertManualFact(input: {
  key: string;
  value: string;
  cf: number;
}): Promise<UiStatePayload> {
  const parsed = ManualFactFormSchema.safeParse({
    key: input.key.trim(),
    value: input.value.trim(),
    cf: input.cf,
  });
  if (!parsed.success) {
    const issue = parsed.error.issues[0]?.message ?? 'Некоректні параметри ручного факту.';
    return Promise.reject(new Error(issue));
  }
  return postAction('/api/dss/fact', parsed.data);
}

export function clearManualFacts(): Promise<UiStatePayload> {
  return postAction('/api/dss/fact/clear');
}
