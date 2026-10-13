// Capture PTX buffer + correlate reads with RPP emit sites.
//
// Strategy:
// 1. Hook NSData/CFData allocation to find candidate buffers >= 10KB
//    (decrypted PTX is typically tens of KB to MB).
// 2. When the converter calls an emit site, log "current buffer ranges
//    accessed since last emit" using a Stalker transform that records
//    memory reads to the PTX buffer.
//
// Output is JSON-lines so a Linux-side script can parse.

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

// Find emit sites (subset that we care about for byte-mapping)
var EMITS = [
  [0x56b94, "MUTESOLO"],
  [0x56b20, "VOLPAN"],
  [0x57fe0, "PEAKCOL"],
  [0x583e4, "ISBUS"],
  [0x56a78, "TRACK_or_ID"],
  [0x56ca4, "MAINSEND"],
  [0x59698, "AUXRECV"],
  [0x5aa58, "NOTES"],
  [0x53954, "TEMPO"],
];

// Track candidate PTX buffer regions.
// We watch for any large allocation (>32KB) — that's the size class of
// decrypted PTX bodies. Keep the LAST few seen — the one that overlaps
// reads at emit time is the live PTX buffer.
var candidates = [];
function recordAlloc(addr, size, source) {
  if (size < 32768 || size > 50 * 1024 * 1024) return;
  candidates.push({addr: addr, size: size, src: source, t: Date.now()});
  if (candidates.length > 16) candidates.shift();
  console.log(JSON.stringify({msg:"alloc", src: source, addr: addr.toString(), size: size}));
}

// Hook malloc/calloc for large sizes
try {
  var malloc = Module.findExportByName("libsystem_malloc.dylib", "malloc");
  if (malloc) {
    Interceptor.attach(malloc, {
      onEnter: function(args) { this.sz = args[0].toInt32(); },
      onLeave: function(retval) {
        if (this.sz >= 32768 && this.sz <= 50 * 1024 * 1024) {
          recordAlloc(retval, this.sz, "malloc");
        }
      }
    });
  }
} catch (e) { console.log(JSON.stringify({msg:"err",hook:"malloc",err:e.toString()})); }

// Hook mmap (PT files might be mmaped)
try {
  var mmap = Module.findExportByName("libsystem_kernel.dylib", "mmap");
  if (mmap) {
    Interceptor.attach(mmap, {
      onEnter: function(args) { this.sz = args[1].toInt32(); },
      onLeave: function(retval) {
        if (this.sz >= 32768) {
          recordAlloc(retval, this.sz, "mmap");
        }
      }
    });
  }
} catch (e) { console.log(JSON.stringify({msg:"err",hook:"mmap",err:e.toString()})); }

// At each emit site, log the call-context. We can't trivially trace
// READS from candidate buffers without Stalker — but we CAN look at
// register values that might be pointers INTO the PTX buffer.
EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  var addr = base.add(off);
  try {
    Interceptor.attach(addr, function() {
      var c = this.context;
      // Collect register values that fall inside any candidate buffer
      var regs = ["x0","x1","x8","x19","x20","x21","x22","x23","x24","x25","x26","x27","x28"];
      var hits = [];
      regs.forEach(function(r){
        var v = c[r];
        if (!v) return;
        var vu = v.toUInt32 ? v.toUInt32() : v.toInt32();
        // Compare against all candidate buffers
        candidates.forEach(function(cand){
          var ca = cand.addr.toString();
          var ci = uint64(ca);
          var ce = uint64(ca).add(cand.size);
          // ptr-in-range check using NativePointer.compare
          if (v.compare(cand.addr) >= 0 && v.compare(cand.addr.add(cand.size)) < 0) {
            var off = v.sub(cand.addr).toInt32();
            hits.push({r: r, buf: cand.addr.toString(), off: off, val: v.toString()});
          }
        });
      });
      console.log(JSON.stringify({msg:"emit", f: feat, hits: hits, regs: {
        x0: c.x0.toString(), x1: c.x1.toString(), x19: c.x19.toString(),
        x20: c.x20.toString(), x21: c.x21.toString(), x22: c.x22.toString(),
        x26: c.x26.toString(), x27: c.x27.toString(), x28: c.x28.toString(),
      }}));
    });
  } catch(e) { console.log(JSON.stringify({msg:"err",hook:feat,err:e.toString()})); }
});

console.log(JSON.stringify({msg:"ready", hooks: EMITS.length}));
