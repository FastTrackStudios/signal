var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

var EMITS = [
  [0x56b94, "MUTESOLO"], [0x56b20, "VOLPAN"], [0x57fe0, "PEAKCOL"],
  [0x583e4, "ISBUS"], [0x56a78, "TRACK_or_ID"], [0x5aa58, "NOTES"],
];
var emitTimeouts = {};
EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  try { Interceptor.attach(base.add(off), function() {
    console.log(JSON.stringify({msg:"emit", f: feat, t: Date.now()}));
  }); } catch(e) {}
});

// Hook subscript stub
try {
  Interceptor.attach(base.add(0x2690d4), {
    onEnter: function(args) {
      this.off = args[0].toInt32();
    },
    onLeave: function(retval) {
      // arm64 ABI: small return in w0 (low 32 bits of x0)
      var v = retval.toInt32() & 0xff;
      console.log(JSON.stringify({msg:"read", off: this.off, val: v}));
    }
  });
} catch(e) { console.log(JSON.stringify({msg:"err",err:e.toString()})); }
console.log(JSON.stringify({msg:"ready"}));
