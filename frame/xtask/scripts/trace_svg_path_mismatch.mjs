#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

function usage() {
  console.error(
    "usage: node xtask/scripts/trace_svg_path_mismatch.mjs --scenario <id> [--node-id <figma-id>] [--path-index <n>]"
  );
}

function parseArgs(argv) {
  const out = { scenario: "", nodeId: "", pathIndex: 0 };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--scenario") out.scenario = argv[++i] ?? "";
    else if (a === "--node-id") out.nodeId = argv[++i] ?? "";
    else if (a === "--path-index") out.pathIndex = Number(argv[++i] ?? "0");
    else throw new Error(`unknown arg: ${a}`);
  }
  if (!out.scenario) throw new Error("--scenario is required");
  if (!Number.isFinite(out.pathIndex) || out.pathIndex < 0) {
    throw new Error("--path-index must be >= 0");
  }
  return out;
}

function firstPath(svgText, index) {
  const regex = /<path d="([^"]+)"/g;
  let m;
  let i = 0;
  while ((m = regex.exec(svgText))) {
    if (i === index) return m[1];
    i++;
  }
  return null;
}

function tokenizePath(d) {
  const out = [];
  let i = 0;
  while (i < d.length) {
    const c = d[i];
    if (/[A-Za-z]/.test(c)) {
      out.push({ k: "c", v: c });
      i++;
      continue;
    }
    if (/[,\s]/.test(c)) {
      i++;
      continue;
    }
    const start = i++;
    while (i < d.length && !/[A-Za-z,\s]/.test(d[i])) i++;
    out.push({ k: "n", v: d.slice(start, i) });
  }
  return out;
}

function canonicalizePath(d) {
  const t = tokenizePath(d);
  let i = 0;
  let cx = 0;
  let cy = 0;
  let sx = 0;
  let sy = 0;
  const out = [];

  const push = (cmd, vals) => {
    out.push({ k: "c", v: cmd });
    for (const v of vals) out.push({ k: "n", v });
  };

  while (i < t.length) {
    const cmd = t[i++].v;
    const rel = cmd === cmd.toLowerCase();
    const u = cmd.toUpperCase();
    const nums = [];
    while (i < t.length && t[i].k !== "c") nums.push(Number(t[i++].v));

    if (u === "M") {
      for (let j = 0; j < nums.length; j += 2) {
        let x = nums[j];
        let y = nums[j + 1];
        if (rel) {
          x += cx;
          y += cy;
        }
        if (j === 0) {
          push("M", [x, y]);
          sx = x;
          sy = y;
        } else {
          push("L", [x, y]);
        }
        cx = x;
        cy = y;
      }
      continue;
    }
    if (u === "L") {
      for (let j = 0; j < nums.length; j += 2) {
        let x = nums[j];
        let y = nums[j + 1];
        if (rel) {
          x += cx;
          y += cy;
        }
        push("L", [x, y]);
        cx = x;
        cy = y;
      }
      continue;
    }
    if (u === "H") {
      for (const n of nums) {
        const x = rel ? cx + n : n;
        push("L", [x, cy]);
        cx = x;
      }
      continue;
    }
    if (u === "V") {
      for (const n of nums) {
        const y = rel ? cy + n : n;
        push("L", [cx, y]);
        cy = y;
      }
      continue;
    }
    if (u === "C") {
      for (let j = 0; j < nums.length; j += 6) {
        let a = nums.slice(j, j + 6);
        if (rel) a = [a[0] + cx, a[1] + cy, a[2] + cx, a[3] + cy, a[4] + cx, a[5] + cy];
        push("C", a);
        cx = a[4];
        cy = a[5];
      }
      continue;
    }
    if (u === "S" || u === "Q") {
      for (let j = 0; j < nums.length; j += 4) {
        let a = nums.slice(j, j + 4);
        if (rel) a = [a[0] + cx, a[1] + cy, a[2] + cx, a[3] + cy];
        push(u, a);
        cx = a[2];
        cy = a[3];
      }
      continue;
    }
    if (u === "T") {
      for (let j = 0; j < nums.length; j += 2) {
        let x = nums[j];
        let y = nums[j + 1];
        if (rel) {
          x += cx;
          y += cy;
        }
        push("T", [x, y]);
        cx = x;
        cy = y;
      }
      continue;
    }
    if (u === "A") {
      for (let j = 0; j < nums.length; j += 7) {
        const a = nums.slice(j, j + 7);
        if (rel) {
          a[5] += cx;
          a[6] += cy;
        }
        push("A", a);
        cx = a[5];
        cy = a[6];
      }
      continue;
    }
    if (u === "Z") {
      out.push({ k: "c", v: "Z" });
      cx = sx;
      cy = sy;
      continue;
    }
    throw new Error(`unsupported path command: ${cmd}`);
  }
  return out;
}

function walk(node, parent, out) {
  const rt = node.relativeTransform ?? [
    [1, 0, 0],
    [0, 1, 0],
  ];
  const localX = rt[0]?.[2] ?? 0;
  const localY = rt[1]?.[2] ?? 0;
  const gx = parent[0] + localX;
  const gy = parent[1] + localY;
  out.push({ node, gx, gy });
  for (const c of node.children ?? []) walk(c, [gx, gy], out);
}

function applyTranslate(tokens, tx, ty) {
  let cmd = "";
  let axis = 0;
  for (const t of tokens) {
    if (t.k === "c") {
      cmd = t.v;
      axis = 0;
      continue;
    }
    const u = cmd;
    let add = 0;
    if (u === "M" || u === "L" || u === "T" || u === "C" || u === "Q" || u === "S") {
      add = axis % 2 === 0 ? tx : ty;
      axis++;
    } else if (u === "A") {
      add = axis === 5 ? tx : axis === 6 ? ty : 0;
      axis = (axis + 1) % 7;
    }
    t.v += add;
  }
}

function round4(v) {
  return Math.round(v * 10000) / 10000;
}

function main() {
  const args = parseArgs(process.argv);
  const scenarioDir = path.join("features/render/tests/parity/scenarios", args.scenario);
  const expectedSvg = fs.readFileSync(path.join(scenarioDir, "expected.svg"), "utf8");
  const exportJson = JSON.parse(
    fs.readFileSync(path.join(scenarioDir, "latest-bridge-export.json"), "utf8")
  );

  const expectedPath = firstPath(expectedSvg, args.pathIndex);
  if (!expectedPath) throw new Error(`expected.svg path index ${args.pathIndex} not found`);
  const expectedCanon = canonicalizePath(expectedPath);

  const all = [];
  walk(exportJson.nodes[0], [0, 0], all);
  const target =
    all.find((e) => args.nodeId && e.node.id === args.nodeId) ??
    all.find((e) => e.node.type === "TEXT" && (e.node.fillGeometry?.length ?? 0) > 0) ??
    null;
  if (!target) throw new Error("no suitable source node found");

  const srcEntry = target.node.fillGeometry?.[args.pathIndex] ?? target.node.fillGeometry?.[0];
  const srcPath = srcEntry?.data ?? srcEntry?.path;
  if (!srcPath) throw new Error("source fillGeometry path missing");

  const sourceCanon = canonicalizePath(srcPath);
  const tx = target.gx - (exportJson.nodes[0].absoluteBoundingBox?.x ?? 0);
  const ty = target.gy - (exportJson.nodes[0].absoluteBoundingBox?.y ?? 0);
  applyTranslate(sourceCanon, tx, ty);

  const mismatches = [];
  for (let i = 0; i < Math.min(sourceCanon.length, expectedCanon.length); i++) {
    const a = sourceCanon[i];
    const b = expectedCanon[i];
    if (a.k !== b.k) {
      mismatches.push({ i, kind: "cmd", source: a.v, expected: b.v });
      continue;
    }
    if (a.k === "n") {
      const as = round4(a.v);
      const bs = round4(b.v);
      if (as !== bs) {
        mismatches.push({
          i,
          kind: "num",
          sourceRaw: a.v,
          expectedRaw: b.v,
          source4dp: as,
          expected4dp: bs,
          delta: a.v - b.v,
        });
      }
    } else if (a.v !== b.v) {
      mismatches.push({ i, kind: "cmd", source: a.v, expected: b.v });
    }
    if (mismatches.length >= 20) break;
  }

  console.log(
    JSON.stringify(
      {
        scenario: args.scenario,
        nodeId: target.node.id,
        nodeName: target.node.name,
        pathIndex: args.pathIndex,
        translate: { tx, ty },
        sampleMismatches: mismatches,
      },
      null,
      2
    )
  );
}

try {
  main();
} catch (err) {
  usage();
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
