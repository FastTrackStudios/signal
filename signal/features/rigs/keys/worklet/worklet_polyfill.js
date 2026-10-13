// AudioWorkletGlobalScope has no TextDecoder/TextEncoder, but wasm-bindgen's
// glue constructs both at module top level — without this polyfill the glue
// module throws during evaluation, registerProcessor never runs, and (Chrome
// quirk) addModule still RESOLVES, so the failure is silent. keys_processor.js
// imports this module FIRST; static-import evaluation order runs it before
// the glue.
//
// UTF-8 only — exactly what wasm-bindgen uses ('utf-8', ignoreBOM, fatal).
//
// Also: no `crypto` in the worklet scope, but the lane build generates track
// GUIDs through uuid→getrandom→crypto.getRandomValues. These are object
// identifiers, not secrets — Math.random is sufficient here.

// And no `performance` — web-time (a transitive wasm clock) panics with
// "`Performance` object not found" the first time anything takes a
// timestamp. Date.now keeps monotonic-enough time for its uses here.
if (typeof globalThis.performance === 'undefined') {
  const t0 = Date.now();
  globalThis.performance = {
    now: () => Date.now() - t0,
    timeOrigin: t0,
  };
}

if (typeof globalThis.crypto === 'undefined' || !globalThis.crypto.getRandomValues) {
  globalThis.crypto = {
    getRandomValues(arr) {
      for (let i = 0; i < arr.length; i++) {
        arr[i] = (Math.random() * 256) | 0;
      }
      return arr;
    },
  };
}

if (typeof globalThis.TextDecoder === 'undefined') {
  globalThis.TextDecoder = class TextDecoder {
    constructor(_label, _opts) {}
    decode(input) {
      if (input === undefined) {
        return '';
      }
      const b = input instanceof Uint8Array ? input : new Uint8Array(input.buffer ?? input);
      let out = '';
      let i = 0;
      const n = b.length;
      while (i < n) {
        const c = b[i];
        let cp;
        if (c < 0x80) {
          cp = c;
          i += 1;
        } else if (c < 0xe0) {
          cp = ((c & 0x1f) << 6) | (b[i + 1] & 0x3f);
          i += 2;
        } else if (c < 0xf0) {
          cp = ((c & 0x0f) << 12) | ((b[i + 1] & 0x3f) << 6) | (b[i + 2] & 0x3f);
          i += 3;
        } else {
          cp = ((c & 0x07) << 18) | ((b[i + 1] & 0x3f) << 12) | ((b[i + 2] & 0x3f) << 6) | (b[i + 3] & 0x3f);
          i += 4;
        }
        if (cp > 0xffff) {
          cp -= 0x10000;
          out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
        } else {
          out += String.fromCharCode(cp);
        }
      }
      return out;
    }
  };
}

if (typeof globalThis.TextEncoder === 'undefined') {
  globalThis.TextEncoder = class TextEncoder {
    encode(s = '') {
      const out = [];
      for (let i = 0; i < s.length; i++) {
        let cp = s.codePointAt(i);
        if (cp > 0xffff) {
          i++; // consumed a surrogate pair
        }
        if (cp < 0x80) {
          out.push(cp);
        } else if (cp < 0x800) {
          out.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
          out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
        } else {
          out.push(
            0xf0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3f),
            0x80 | ((cp >> 6) & 0x3f),
            0x80 | (cp & 0x3f),
          );
        }
      }
      return new Uint8Array(out);
    }
    encodeInto(s, view) {
      const bytes = this.encode(s);
      const written = Math.min(bytes.length, view.length);
      view.set(bytes.subarray(0, written));
      return { read: s.length, written };
    }
  };
}
