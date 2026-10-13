// Trace every Data subscript read (Foundation.Data[i] -> UInt8).
//
// The converter parses PTX bytes via Foundation.Data subscript:
//   __s10Foundation4DataV15_RepresentationOys5UInt8VSicig
// This function reads one byte at offset i from the Data buffer.
//
// Each call gives us: (offset, returned_byte). Correlated with emit
// hooks, we can map "feature X reads byte at offset Y = value Z".

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

// Buffer that the Data points to — track the most recently-seen base.
var lastDataBase = null;

// Find the subscript function symbol. Try the dynamic library first;
// fall back to the local PLT stub at base+0x2690d4 (found in Ghidra).
var subSym = null;
try {
  subSym = Module.findExportByName("libswiftFoundation.dylib",
    "$s10Foundation4DataV15_RepresentationOys5UInt8VSicig");
  if (!subSym) {
    subSym = Module.findExportByName(null,
      "_$s10Foundation4DataV15_RepresentationOys5UInt8VSicig");
  }
} catch (e) {}

if (!subSym) {
  // Stub in main binary (resolves to the dynamic Foundation function).
  subSym = base.add(0x2690d4);
}

console.log(JSON.stringify({msg:"sub_at", addr: subSym ? subSym.toString() : "null"}));

// Recent reads — keep a small ring buffer
var recentReads = [];
var MAX_RECENT = 200;

function recordRead(offset, value) {
  recentReads.push({off: offset, val: value, t: Date.now()});
  if (recentReads.length > MAX_RECENT) recentReads.shift();
}

if (subSym) {
  Interceptor.attach(subSym, {
    onEnter: function(args) {
      // ARM64: args are in x0, x1, x2. Subscript: index in x0 maybe?
      // Conservative: just record all args
      this.off = args[0].toInt32();
    },
    onLeave: function(retval) {
      var val = retval.toInt32() & 0xff;
      recordRead(this.off, val);
    }
  });
}

// Hook emit sites — at each emit, dump the most recent reads
var EMITS = [
  [0x56b94, "MUTESOLO"],
  [0x56b20, "VOLPAN"],
  [0x57fe0, "PEAKCOL"],
  [0x583e4, "ISBUS"],
  [0x56a78, "TRACK_or_ID"],
  [0x5aa58, "NOTES"],
  [0x65dbc, "FADEIN"],
  [0x65f4c, "FADEOUT"],
];

EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  var addr = base.add(off);
  try {
    Interceptor.attach(addr, function() {
      // Snapshot last N reads
      var tail = recentReads.slice(-20);
      console.log(JSON.stringify({msg:"emit", f: feat, recent_reads: tail}));
    });
  } catch(e) {}
});

console.log(JSON.stringify({msg:"ready"}));
