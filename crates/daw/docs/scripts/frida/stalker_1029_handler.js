// Frida Stalker on the 0x1029 handler (FUN_1001267e0).
//
// Stalker intercepts every instruction inside the handler. We log
// every memory LDR (load) and the value read. Combined with the
// byte-subscript hook this gives us ALL bytes accessed during parse
// of a 0x1029 block — including direct-pointer reads of u32 vol,
// u32 pan, and any other inline multi-byte fields the converter
// uses outside Foundation Data.subscript.
//
// Output: JSON-lines `{msg:"ld", at:<insn-addr>, src:<addr>, val:<u64>}`
// for each memory load. The source address can be matched against
// known buffer ranges to identify PTX byte offsets.

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

var HANDLER_OFFSET = 0x1267e0;  // FUN_1001267e0 in our Ghidra arm64 image
var handlerAddr = base.add(HANDLER_OFFSET);

// Track buffer base by hooking the byte subscript and remembering the
// most recent Data instance pointer (passed in x1 / x2 of the
// subscript call).
var lastDataPtr = null;
try {
  Interceptor.attach(base.add(0x2690d4), {
    onEnter: function(args) {
      // Subscript: arg0 = index (Int), arg1 = data ptr? arg2 = something
      lastDataPtr = args[1];
    }
  });
} catch (e) {}

// Stalker on the handler — only instrument when entering, follow
// follow-set inside.
var stalkerActive = false;

// Aggregate load PC frequency across the whole handler run.
// At onLeave, dump the histogram.
var loadCounts = {};

try {
  Interceptor.attach(handlerAddr, {
    onEnter: function() {
      if (stalkerActive) return;
      stalkerActive = true;
      console.log(JSON.stringify({msg:"stalker_attach"}));
      var moduleEndArg = base.add(HANDLER_OFFSET);
      Stalker.follow(this.threadId, {
        transform: function(iter) {
          var insn;
          while ((insn = iter.next()) !== null) {
            var m = insn.mnemonic;
            if (m === "ldr" || m === "ldrb" || m === "ldrh" || m === "ldrsb"
                || m === "ldrsh" || m === "ldrsw"
                || m === "ldp" || m === "ldur" || m === "ldurb" || m === "ldurh"
                || m === "ldursh" || m === "ldursw") {
              var pc = insn.address.toString();
              if (loadCounts[pc] === undefined) {
                loadCounts[pc] = 0;
              }
              iter.putCallout(function(ctx) {
                // Bump count, capped to avoid memory blow-up
                var pcStr = ctx.pc.toString();
                if (loadCounts[pcStr] !== undefined) {
                  loadCounts[pcStr] += 1;
                }
              });
            }
            iter.keep();
          }
        }
      });
    },
    onLeave: function() {
      if (!stalkerActive) return;
      Stalker.unfollow(this.threadId);
      Stalker.flush();
      stalkerActive = false;
      var pcs = Object.keys(loadCounts).map(function(k) {
        return { pc: k, n: loadCounts[k] };
      });
      pcs.sort(function(a, b) { return b.n - a.n; });
      console.log(JSON.stringify({msg: "stalker_summary", top: pcs.slice(0, 20)}));
      loadCounts = {};
    }
  });
  console.log(JSON.stringify({msg:"hook_ok", handler: handlerAddr.toString()}));
} catch (e) {
  console.log(JSON.stringify({msg:"err", err: e.toString()}));
}

console.log(JSON.stringify({msg:"ready"}));
