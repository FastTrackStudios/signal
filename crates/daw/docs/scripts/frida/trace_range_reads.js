// Hook Foundation Data range-subscript: `data[r..s]` slice reads.
// Mangled: _$s10Foundation4DataV15_RepresentationOyACSnySiGcig
// PLT stub in main binary at base + 0x2690c8.

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

try {
  Interceptor.attach(base.add(0x2690c8), {
    onEnter: function(args) {
      // Range subscript: arg0 = lower, arg1 = upper, then Data self?
      this.lo = args[0].toInt32();
      this.hi = args[1].toInt32();
    },
    onLeave: function(retval) {
      console.log(JSON.stringify({msg:"range", lo: this.lo, hi: this.hi}));
    }
  });
  console.log(JSON.stringify({msg:"hook_ok"}));
} catch (e) {
  console.log(JSON.stringify({msg:"err", err: e.toString()}));
}

// Also hook every RPP-emit site for correlation
var EMITS = [
  [0x56b94, "MUTESOLO"], [0x56b20, "VOLPAN"], [0x57fe0, "PEAKCOL"],
  [0x583e4, "ISBUS"], [0x56a78, "TRACK_or_ID"], [0x59698, "AUXRECV"],
  [0x5aa58, "NOTES"], [0x53954, "TEMPO"], [0x53ef0, "MARKER"],
  [0x65dbc, "FADEIN"], [0x65f4c, "FADEOUT"], [0x750bc, "FXCHAIN"],
];
EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  try {
    Interceptor.attach(base.add(off), function() {
      console.log(JSON.stringify({msg:"emit", f: feat}));
    });
  } catch(e) {}
});

console.log(JSON.stringify({msg:"ready"}));
