// Universal RPP-emit logger. For each known emit site, dumps the calling
// context (x0..x28) when execution reaches that instruction. Tags each
// event with the most recently-seen track name (from NAME emits or the
// per-track function entry).

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
console.log("base " + pt.base);

var EMITS = [
  // [load-addr offset, feature]
  [0x56b94, "MUTESOLO"],
  [0x56b20, "VOLPAN"],
  [0x53ef0, "MARKER"],
  [0x57fe0, "PEAKCOL"],
  [0x583e4, "ISBUS"],
  [0x56a78, "TRACK_or_ID"],
  [0x56bec, "NCHAN"],
  [0x56ca4, "MAINSEND"],
  [0x59698, "AUXRECV"],
  [0x750bc, "FXCHAIN"],
  [0x53954, "TEMPO"],
  [0x571c4, "POSITION"],
  [0x57248, "LENGTH"],
  [0x573ac, "SOFFS"],
  [0x65dbc, "FADEIN"],
  [0x65f4c, "FADEOUT"],
  [0x2355b8, "PLAYRATE"],
  [0x573fc, "CHANMODE"],
  [0x57558, "SOURCE"],
  [0x537e4, "SAMPLERATE"],
  [0x56d3c, "FREEMODE"],
  [0x5aa58, "NOTES"],
  [0x61400, "MUTESOLO_emit2"],  // second site we already validated
  [0x60b28, "FN_60b28_entry"],  // small track-emit function entry
];

var lastTrackName = "?";
function decodeSmall(lo, hi) {
  try {
    var tag = hi.shr(56).and(0xff).toNumber();
    if ((tag & 0xE0) !== 0xE0) return null;
    var len = tag & 0xf;
    var s = "";
    for (var i = 0; i < Math.min(len, 8); i++) {
      var c = lo.shr(i*8).and(0xff).toNumber();
      if (c >= 0x20 && c < 0x7F) s += String.fromCharCode(c);
    }
    for (var i = 0; i < Math.max(0, len-8); i++) {
      var c = hi.shr(i*8).and(0xff).toNumber();
      if (c >= 0x20 && c < 0x7F) s += String.fromCharCode(c);
    }
    return s;
  } catch(e) { return null; }
}

// Track name probe: any time x0 looks like a Swift small-string with the
// length matching a track name, update lastTrackName.
EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  var addr = pt.base.add(off);
  try {
    Interceptor.attach(addr, function() {
      var c = this.context;
      // Try to pull track name from common register positions
      var n0 = decodeSmall(c.x0, c.x1);
      if (n0 && n0.length >= 2 && /^[A-Za-z0-9_ .]/.test(n0[0])) {
        // Only update if it looks like an alphabetic track name (not just numbers)
        if (/[A-Za-z]/.test(n0)) lastTrackName = n0;
      }
      var dump = {
        f: feat,
        t: lastTrackName,
        x0: c.x0.toString(),
        x8: c.x8.toString(),
        x19: c.x19.toString(),
        x20: c.x20.toString(),
        x21: c.x21.toString(),
        x22: c.x22.toString(),
        x23: c.x23.toString(),
        x24: c.x24.toString(),
        x25: c.x25.toString(),
        x26: c.x26.toString(),
        x27: c.x27.toString(),
        x28: c.x28.toString(),
      };
      console.log("EMIT " + JSON.stringify(dump));
    });
  } catch(e) { console.log("hook err " + feat + ": " + e); }
});

console.log("READY: " + EMITS.length + " hooks armed");
