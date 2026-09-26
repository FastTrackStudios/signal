// node profile.mjs <profileIndex> <patchIndex>
import fs from 'fs';
import init, * as wasm from './signal_guitar_worklet.js';
await init({ module_or_path: fs.readFileSync(new URL('./signal_guitar_worklet_bg.wasm', import.meta.url)) });
const b = JSON.parse(fs.readFileSync(new URL('./rig/rig.json', import.meta.url)));
const [pi, xi] = process.argv.slice(2).map(Number);
const patch = b.profiles[pi].patches[xi];
for (const bl of patch.blocks) for (const k of [bl.nam, bl.ir]) if (k) wasm.installAsset(k, fs.readFileSync(new URL('./rig/' + k, import.meta.url)));
const rows = JSON.parse(wasm.profileChain(JSON.stringify(patch.blocks), 48000, 400));
let on = 0, all = 0;
for (const r of rows) { all += r.us; if (!r.bypassed) on += r.us; console.log(`${r.us.toFixed(1).padStart(8)} µs  ${r.bypassed ? '(off) ' : '      '}${r.name}`); }
console.log(`playing: ${on.toFixed(0)} µs of 2667 (${(on/26.67).toFixed(0)}%) · everything on: ${all.toFixed(0)} µs`);
