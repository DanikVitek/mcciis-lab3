import { createContext } from 'svelte';
import { createSubscriber } from 'svelte/reactivity';
import { fetchUiState, formatApiError } from '#lib/api.ts';
import type { NoticeKind } from '#lib/notify-context.ts';
import { AgentUpdateEventSchema, type UiStatePayload } from '#lib/schemas.ts';

export type WsStatus = 'connecting' | 'connected' | 'reconnecting';

export class UiStateStore {
  #payload = $state.raw<UiStatePayload | null>(null);
  #isSyncing = $state(false);
  #initialPromise = $state.raw<Promise<UiStatePayload>>(this.#startInitialFetch());

  #wsStatus: WsStatus = 'connecting';
  #livePollInProgress = false;
  #subscribeWs: () => void;

  #refreshInFlight: Promise<void> | null = null;
  #lastAppliedAt = 0;
  #onNotify: ((kind: NoticeKind, text: string) => void) | undefined;

  constructor(onNotify?: (kind: NoticeKind, text: string) => void) {
    this.#onNotify = onNotify;

    this.#subscribeWs = createSubscriber((update) => {
      if (typeof window === 'undefined' || typeof WebSocket === 'undefined') {
        return;
      }

      let closed = false;
      let socket: WebSocket | null = null;
      let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
      let reconnectAttempt = 0;

      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = `${protocol}//${window.location.host}/api/ws`;

      const setStatus = (next: WsStatus) => {
        if (this.#wsStatus !== next) {
          this.#wsStatus = next;
          update();
        }
      };

      const setLivePoll = (running: boolean) => {
        if (this.#livePollInProgress !== running) {
          this.#livePollInProgress = running;
          update();
        }
      };

      const scheduleReconnect = () => {
        if (closed) return;
        setStatus('reconnecting');
        const delayMs = Math.min(15_000, 1000 * 2 ** reconnectAttempt);
        reconnectAttempt = Math.min(reconnectAttempt + 1, 5);
        reconnectTimer = setTimeout(openSocket, delayMs);
      };

      const openSocket = () => {
        if (closed) return;
        setStatus(reconnectAttempt === 0 ? 'connecting' : 'reconnecting');

        try {
          socket = new WebSocket(wsUrl);
        } catch {
          scheduleReconnect();
          return;
        }

        socket.onopen = () => {
          if (closed) return;
          reconnectAttempt = 0;
          setStatus('connected');
        };

        socket.onmessage = (msgEvent) => {
          if (closed || typeof msgEvent.data !== 'string') return;
          let raw: unknown;
          try {
            raw = JSON.parse(msgEvent.data);
          } catch {
            return;
          }
          const parsed = AgentUpdateEventSchema.safeParse(raw);
          if (!parsed.success) return;

          const ev = parsed.data;
          setLivePoll(ev.poll_in_progress);

          if (ev.kind === 'connected') {
            if (this.#payload && ev.poll_count !== this.#payload.snapshot.poll_count) {
              void this.refresh();
            }
            return;
          }

          if (ev.kind === 'poll_started') {
            return;
          }

          if (ev.kind === 'poll_completed') {
            void this.refresh().then(() => {
              const dur =
                ev.last_poll_duration_ms !== null ? ` за ${ev.last_poll_duration_ms} мс` : '';
              this.#onNotify?.(
                'info',
                `Цикл #${ev.poll_count} завершено${dur}: новин — ${ev.articles_count}, рішень ІСППР — ${ev.decisions_count}.`,
              );
            });
            return;
          }

          if (ev.kind === 'state_changed') {
            // Уникаємо зайвого повторного запиту, якщо щойно застосували відповідь власного POST
            if (Date.now() - this.#lastAppliedAt > 350) {
              void this.refresh();
            }
          }
        };

        socket.onclose = () => {
          socket = null;
          scheduleReconnect();
        };

        socket.onerror = () => {
          socket?.close();
        };
      };

      openSocket();

      return () => {
        closed = true;
        if (reconnectTimer) clearTimeout(reconnectTimer);
        socket?.close();
        socket = null;
      };
    });
  }

  get payload(): UiStatePayload | null {
    this.#subscribeWs();
    return this.#payload;
  }

  get wsStatus(): WsStatus {
    this.#subscribeWs();
    return this.#wsStatus;
  }

  get isSyncing(): boolean {
    return this.#isSyncing;
  }

  get isPollRunning(): boolean {
    this.#subscribeWs();
    return this.#livePollInProgress || (this.#payload?.snapshot.poll_in_progress ?? false);
  }

  load(): UiStatePayload | Promise<UiStatePayload> {
    this.#subscribeWs();
    return this.#payload ?? this.#initialPromise;
  }

  #startInitialFetch(): Promise<UiStatePayload> {
    const promise = fetchUiState();
    void promise.then(
      (data) => {
        this.#payload = data;
        this.#livePollInProgress = data.snapshot.poll_in_progress;
        this.#lastAppliedAt = Date.now();
      },
      () => {
        // Error is handled by <svelte:boundary> failed snippet
      },
    );
    return promise;
  }

  retry(): void {
    this.#payload = null;
    this.#initialPromise = this.#startInitialFetch();
  }

  applyPayload(next: UiStatePayload): void {
    this.#payload = next;
    this.#livePollInProgress = next.snapshot.poll_in_progress;
    this.#lastAppliedAt = Date.now();
  }

  async refresh(): Promise<void> {
    if (this.#refreshInFlight) {
      return this.#refreshInFlight;
    }

    this.#isSyncing = true;
    this.#refreshInFlight = fetchUiState()
      .then((next) => {
        this.applyPayload(next);
      })
      .catch((err: unknown) => {
        this.#onNotify?.('error', formatApiError(err, 'Не вдалося синхронізувати стан із сервером.'));
      })
      .finally(() => {
        this.#isSyncing = false;
        this.#refreshInFlight = null;
      });

    return this.#refreshInFlight;
  }
}

export const [getUiState, setUiState] = createContext<UiStateStore>();
