import { createServer } from 'node:http';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';

const PORT = Number(process.env.FTS_FIGMA_BRIDGE_PORT || 43123);
const HOST = process.env.FTS_FIGMA_BRIDGE_HOST || '';
const OUT_PATH = process.env.FTS_FIGMA_OUT_PATH || '/tmp/fts-figma-export.json';
const PARITY_ROOT = process.env.FTS_PARITY_ROOT
  || '/Users/codywright/Documents/Development/FastTrackStudio/frame/features/render/tests/parity/scenarios';

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type',
};

function sendJson(res, statusCode, payload) {
  res.writeHead(statusCode, {
    'content-type': 'application/json',
    ...CORS_HEADERS,
  });
  res.end(JSON.stringify(payload));
}

const server = createServer(async (req, res) => {
  const requestUrl = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
  console.log(`[fts-figma-bridge] request ${req.method || 'UNKNOWN'} ${requestUrl.pathname}`);

  if (req.method === 'OPTIONS') {
    res.writeHead(204, CORS_HEADERS);
    res.end();
    return;
  }

  if (req.method === 'GET' && requestUrl.pathname === '/health') {
    const source = requestUrl.searchParams.get('source') || 'unknown';
    console.log(`[fts-figma-bridge] health check from ${source}`);
    sendJson(res, 200, {
      ok: true,
      service: 'fts-figma-bridge',
      source,
      now: new Date().toISOString(),
    });
    return;
  }

  if (req.method !== 'POST' || (requestUrl.pathname !== '/figma-export' && requestUrl.pathname !== '/figma-parity-pack')) {
    sendJson(res, 404, { ok: false, error: 'not found' });
    return;
  }

  try {
    const chunks = [];
    for await (const chunk of req) {
      chunks.push(chunk);
    }

    const body = Buffer.concat(chunks);
    if (body.length === 0) {
      sendJson(res, 400, { ok: false, error: 'empty payload' });
      return;
    }

    const parsed = JSON.parse(body.toString('utf8'));
    if (requestUrl.pathname === '/figma-export') {
      await writeFile(OUT_PATH, body);
      console.log(`[fts-figma-bridge] wrote JSON export: ${OUT_PATH} (${body.length} bytes)`);
      sendJson(res, 200, {
        ok: true,
        path: OUT_PATH,
        bytes: body.length,
      });
      return;
    }

    const scenarios = Array.isArray(parsed?.scenarios) ? parsed.scenarios : [];
    const root = typeof parsed?.parityRootDir === 'string' && parsed.parityRootDir.trim().length > 0
      ? parsed.parityRootDir.trim()
      : PARITY_ROOT;
    let written = 0;
    let jsonWritten = 0;
    let jsonFallbackWritten = 0;
    for (const scenario of scenarios) {
      if (!scenario || typeof scenario !== 'object') {
        continue;
      }
      const id = typeof scenario.id === 'string' ? scenario.id : '';
      const exportJson = scenario.exportJson;
      const expectedPngBase64 = typeof scenario.expectedPngBase64 === 'string'
        ? scenario.expectedPngBase64
        : '';
      const expectedSvgBase64 = typeof scenario.expectedSvgBase64 === 'string'
        ? scenario.expectedSvgBase64
        : '';
      if (!id || !expectedPngBase64) {
        continue;
      }
      const scenarioDir = join(root, id);
      const exportPath = join(scenarioDir, 'latest-bridge-export.json');
      const expectedPngPath = join(scenarioDir, 'expected.png');
      const expectedSvgPath = join(scenarioDir, 'expected.svg');
      await mkdir(dirname(exportPath), { recursive: true });
      if (exportJson) {
        await writeFile(exportPath, `${JSON.stringify(exportJson, null, 2)}\n`, 'utf8');
        jsonWritten += 1;
      } else {
        // Keep parity pipeline file layout deterministic even when source plugin
        // does not provide renderer-ready export JSON.
        const fallback = {
          schema: 'fts.figma.parity-pack/fallback-v1',
          generatedAt: new Date().toISOString(),
          warning: 'Scenario did not include exportJson; this file is metadata-only.',
          scenario: {
            id,
            frameName: typeof scenario.frameName === 'string' ? scenario.frameName : undefined,
          },
        };
        await writeFile(exportPath, `${JSON.stringify(fallback, null, 2)}\n`, 'utf8');
        jsonFallbackWritten += 1;
      }
      await writeFile(expectedPngPath, Buffer.from(expectedPngBase64, 'base64'));
      if (expectedSvgBase64) {
        await writeFile(expectedSvgPath, Buffer.from(expectedSvgBase64, 'base64'));
      }
      written += 1;
    }

    sendJson(res, 200, {
      ok: true,
      path: root,
      bytes: body.length,
      writtenScenarios: written,
      jsonWritten,
      jsonFallbackWritten,
    });
    console.log(
      `[fts-figma-bridge] wrote parity pack: ${written} scenario(s), json=${jsonWritten}, fallback-json=${jsonFallbackWritten} -> ${root}`,
    );
  } catch (error) {
    console.error('[fts-figma-bridge] request failed:', error instanceof Error ? error.message : String(error));
    sendJson(res, 500, {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
});

server.listen(PORT, HOST || undefined, () => {
  const hostLabel = HOST || '0.0.0.0/::';
  console.log(`[fts-figma-bridge] listening on http://${hostLabel}:${PORT}`);
  console.log(`[fts-figma-bridge] /figma-export -> ${OUT_PATH}`);
  console.log(`[fts-figma-bridge] /figma-parity-pack -> ${PARITY_ROOT}`);
});
