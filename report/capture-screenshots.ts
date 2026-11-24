#!/usr/bin/env -S deno run --allow-run --allow-read --allow-write --allow-net --allow-env
/**
 * Reproducible script to capture full-page screenshots of the live
 * Web-Agent (Rust Axum backend serving static SvelteKit SPA + JSON API) for Lab 3 report.
 *
 * Uses headless Firefox driven via WebDriver BiDi over WebSocket so that each SPA route
 * finishes client-side hydration and API fetching before taking the full-document screenshot.
 */

interface CurrencyRate {
  cc: string;
  rate: number;
  exchangedate: string;
}

interface NewsArticle {
  id: string;
  title: string;
  relevance_score: number;
  cves: string[];
}

interface AgentSnapshot {
  poll_count: number;
  last_poll_duration_ms: number | null;
  sites: Array<{ changed: boolean }>;
  feeds: unknown[];
  articles: NewsArticle[];
  currencies: CurrencyRate[];
  agent_facts: Record<string, unknown>;
  inference: {
    iterations: number;
    fired_rule_ids: string[];
    inferred_facts: Record<string, { value: unknown }>;
  };
}

interface BiDiResponse {
  id?: number;
  type: string;
  result?: Record<string, unknown>;
  error?: string;
  message?: string;
}

const SCRIPT_DIR = new URL('.', import.meta.url);
const ROOT_DIR = new URL('..', SCRIPT_DIR);
const SCREENSHOTS_DIR = new URL('./screenshots/', SCRIPT_DIR);

const SERVER_URL = 'http://127.0.0.1:8080';
const BIDI_PORT = 9222;
const VIEWPORT_WIDTH = 1440;
const VIEWPORT_HEIGHT = 900;

function urlToPath(url: URL): string {
  return decodeURIComponent(url.pathname);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function decodeBase64(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

async function waitForFirstPoll(timeoutMs = 60_000): Promise<AgentSnapshot> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${SERVER_URL}/api/state`);
      if (res.ok) {
        const snapshot = (await res.json()) as AgentSnapshot;
        if (snapshot.poll_count >= 1) {
          return snapshot;
        }
      }
    } catch {
      // Server still starting up
    }
    await sleep(1000);
  }
  throw new Error('Timed out waiting for Rust server to finish first poll.');
}

async function readPngDimensions(path: string): Promise<{ width: number; height: number }> {
  const bytes = await Deno.readFile(path);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    width: view.getUint32(16, false),
    height: view.getUint32(20, false),
  };
}

class FirefoxBiDiClient {
  #ws: WebSocket;
  #nextId = 1;
  #pending = new Map<
    number,
    { resolve: (value: Record<string, unknown>) => void; reject: (reason: Error) => void }
  >();

  private constructor(ws: WebSocket) {
    this.#ws = ws;
    this.#ws.onmessage = (event) => {
      const payload = JSON.parse(String(event.data)) as BiDiResponse;
      if (typeof payload.id !== 'number') return;
      const waiter = this.#pending.get(payload.id);
      if (!waiter) return;
      this.#pending.delete(payload.id);
      if (payload.type === 'error') {
        waiter.reject(
          new Error(`WebDriver BiDi error (${payload.error ?? 'unknown'}): ${payload.message ?? ''}`),
        );
      } else {
        waiter.resolve(payload.result ?? {});
      }
    };
    this.#ws.onerror = () => {
      for (const waiter of this.#pending.values()) {
        waiter.reject(new Error('WebDriver BiDi WebSocket error'));
      }
      this.#pending.clear();
    };
  }

  static async connect(wsUrl: string, timeoutMs = 20_000): Promise<FirefoxBiDiClient> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const ws = await new Promise<WebSocket>((resolve, reject) => {
          const socket = new WebSocket(wsUrl);
          socket.onopen = () => resolve(socket);
          socket.onerror = () => reject(new Error('connect failed'));
        });
        return new FirefoxBiDiClient(ws);
      } catch {
        await sleep(250);
      }
    }
    throw new Error(`Timed out connecting to Firefox WebDriver BiDi at ${wsUrl}`);
  }

  send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    const id = this.#nextId++;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async close(): Promise<void> {
    try {
      await this.send('session.end', {});
    } catch {
      // Ignore shutdown errors
    }
    this.#ws.close();
  }
}

async function waitForSelector(
  bidi: FirefoxBiDiClient,
  context: string,
  selector: string,
  timeoutMs = 15_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  const expression = `Boolean(document.querySelector(${JSON.stringify(selector)}))`;
  while (Date.now() < deadline) {
    const evalRes = await bidi.send('script.evaluate', {
      expression,
      target: { context },
      awaitPromise: false,
    });
    const value = (evalRes.result as { value?: unknown } | undefined)?.value;
    if (value === true) {
      // Allow fonts and layout to settle before screenshot
      await sleep(350);
      return;
    }
    await sleep(150);
  }
  throw new Error(`Timed out waiting for selector "${selector}" to render in SPA.`);
}

async function stopProcess(proc: Deno.ChildProcess | null): Promise<void> {
  if (!proc) return;
  try {
    proc.kill('SIGTERM');
  } catch {
    // Already exited
  }
  await proc.status.catch(() => undefined);
}

async function main(): Promise<void> {
  const rootPath = urlToPath(ROOT_DIR).replace(/\/$/, '');
  const screenshotsPath = urlToPath(SCREENSHOTS_DIR);
  await Deno.mkdir(screenshotsPath, { recursive: true });

  const tmpDir = await Deno.makeTempDir({ prefix: 'lab3-report-' });
  const ffProfile = `${tmpDir}/ff-profile`;
  await Deno.mkdir(ffProfile, { recursive: true });

  const baseConfig = await Deno.readTextFile(`${rootPath}/config.toml`);
  const tempConfigPath = `${tmpDir}/config.toml`;
  const tempConfig = baseConfig
    .replace(/^state_path\s*=\s*.*$/m, `state_path = "${tmpDir}/state.json"`)
    .replace(/^dir\s*=\s*.*$/m, `dir = "${tmpDir}/logs"`);
  await Deno.writeTextFile(tempConfigPath, tempConfig);

  let rustProc: Deno.ChildProcess | null = null;
  let firefoxProc: Deno.ChildProcess | null = null;
  let bidi: FirefoxBiDiClient | null = null;

  try {
    console.log('Starting Rust server (web-agent-server, serving JSON API + static SPA)...');
    rustProc = new Deno.Command(`${rootPath}/target/debug/web-agent-server`, {
      args: [tempConfigPath],
      cwd: rootPath,
      stdout: 'null',
      stderr: 'null',
    }).spawn();

    const snapshot = await waitForFirstPoll();

    const spaResp = await fetch(`${SERVER_URL}/`);
    if (!spaResp.ok) {
      throw new Error(`Expected 200 for SPA root /, got ${spaResp.status}`);
    }
    const spaHtml = await spaResp.text();
    console.log(`Static SPA verified: ${spaHtml.length} bytes served directly by Rust Axum`);

    const topArticle = snapshot.articles[0];
    if (!topArticle) {
      throw new Error('No matched articles available after first poll.');
    }

    console.log('--- Live Snapshot Metrics ---');
    console.log(
      `poll_count=${snapshot.poll_count}, duration_ms=${snapshot.last_poll_duration_ms}`,
    );
    console.log(
      `sites=${snapshot.sites.length} (changed=${snapshot.sites.filter((s) => s.changed).length}), feeds=${snapshot.feeds.length}, articles=${snapshot.articles.length}`,
    );
    console.log(
      'currencies:',
      snapshot.currencies.map((c) => `${c.cc}=${c.rate} (${c.exchangedate})`).join(', '),
    );
    console.log(
      `agent_facts=${Object.keys(snapshot.agent_facts).length}, inferred_facts=${Object.keys(snapshot.inference.inferred_facts).length}, iterations=${snapshot.inference.iterations}`,
    );
    console.log(`fired_rules: ${snapshot.inference.fired_rule_ids.join(', ')}`);
    console.log(
      `operational_mode: ${String(snapshot.inference.inferred_facts.operational_mode?.value ?? 'none')}`,
    );
    console.log(
      `top_article: id=${topArticle.id}, score=${topArticle.relevance_score}, cves=[${topArticle.cves.join(', ')}], title="${topArticle.title}"`,
    );

    console.log('Starting headless Firefox with WebDriver BiDi...');
    firefoxProc = new Deno.Command('firefox', {
      args: [
        '--headless',
        '--no-remote',
        '-profile',
        ffProfile,
        `--remote-debugging-port=${BIDI_PORT}`,
      ],
      stdout: 'null',
      stderr: 'null',
    }).spawn();

    bidi = await FirefoxBiDiClient.connect(`ws://127.0.0.1:${BIDI_PORT}/session`);
    await bidi.send('session.new', { capabilities: {} });

    const tree = await bidi.send('browsingContext.getTree', {});
    const contexts = tree.contexts as Array<{ context: string }> | undefined;
    const contextId = contexts?.[0]?.context;
    if (!contextId) {
      throw new Error('Failed to obtain Firefox browsing context.');
    }

    await bidi.send('browsingContext.setViewport', {
      context: contextId,
      viewport: { width: VIEWPORT_WIDTH, height: VIEWPORT_HEIGHT },
      devicePixelRatio: 1,
    });

    const targets: Array<{ name: string; url: string; readySelector: string }> = [
      { name: 'index.png', url: `${SERVER_URL}/`, readySelector: '.hero-title' },
      {
        name: 'article.png',
        url: `${SERVER_URL}/article/${encodeURIComponent(topArticle.id)}`,
        readySelector: '.article-title',
      },
      { name: 'dss.png', url: `${SERVER_URL}/dss`, readySelector: '.rule-card' },
      { name: 'conf.png', url: `${SERVER_URL}/conf`, readySelector: '.summary-grid' },
    ];

    console.log('--- Capturing Full-Page Screenshots ---');
    for (const target of targets) {
      const outPath = `${screenshotsPath}${target.name}`;
      await bidi.send('browsingContext.navigate', {
        context: contextId,
        url: target.url,
        wait: 'complete',
      });
      await waitForSelector(bidi, contextId, target.readySelector);

      const shot = await bidi.send('browsingContext.captureScreenshot', {
        context: contextId,
        origin: 'document',
      });
      const base64Data = shot.data;
      if (typeof base64Data !== 'string' || base64Data.length === 0) {
        throw new Error(`Empty screenshot payload returned for ${target.url}`);
      }
      await Deno.writeFile(outPath, decodeBase64(base64Data));
      const { width, height } = await readPngDimensions(outPath);
      console.log(`  ${target.name}: ${width}x${height} px (${target.url})`);
    }
  } finally {
    if (bidi) {
      await bidi.close();
    }
    await stopProcess(firefoxProc);
    await stopProcess(rustProc);
    await Deno.remove(tmpDir, { recursive: true }).catch(() => undefined);
  }
}

if (import.meta.main) {
  await main();
}
