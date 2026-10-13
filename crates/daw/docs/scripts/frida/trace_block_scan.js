// Hook the block-parse helper FUN_100175f6c to capture every (CT, position)
// pair as the converter scans the decrypted PTX buffer.
//
// FUN_100175f6c signature (from Ghidra):
//   x0 = base of data (or NSData wrap)
//   x1 = length info
//   x2 = start scan offset
//   x3 = end scan limit
//   w4 = magic byte (0x5A for PT)
//   w5 = CT low byte
//   w6 = CT high byte
//   x7 = output array pointer
// Output array entries (28 bytes each):
//   +0x20: block start offset (u64)
//   +0x28: block_type (u16)
//   +0x2c: block_size (u32)
//   +0x30: CT (u16)

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

// Also hook each known emit site so we can correlate emits with the
// last block-scan calls.
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
  [0x53ef0, "MARKER"],
  [0x65dbc, "FADEIN"],
  [0x65f4c, "FADEOUT"],
  [0x750bc, "FXCHAIN"],
];

// Hook FUN_100175f6c at offset 0x175f6c
try {
  var blockScan = base.add(0x175f6c);
  Interceptor.attach(blockScan, {
    onEnter: function(args) {
      var c = this.context;
      // Args per the decomp: param_5 (w4)=magic, param_6 (w5)=CT_lo, param_7 (w6)=CT_hi
      var magic = c.x4.toInt32() & 0xff;
      var ct_lo = c.x5.toInt32() & 0xff;
      var ct_hi = c.x6.toInt32() & 0xff;
      var ct = (ct_hi << 8) | ct_lo;
      this.ct = ct;
      this.start_offset = c.x2.toInt32();
      this.out_array = c.x7;
      // The PTX buffer base is in x0 — capture it.
      var buf = c.x0;
      console.log(JSON.stringify({
        msg: "scan_enter",
        ct: "0x" + ct.toString(16),
        magic: "0x" + magic.toString(16),
        start: this.start_offset,
        buf: buf.toString(),
        out: this.out_array.toString()
      }));
    },
    onLeave: function(retval) {
      // Try to read entries written to the output array
      try {
        var arr = this.out_array;
        if (arr.isNull()) return;
        // First u64 at offset 0x10 in the array struct seems to be the count.
        // Empirically check pattern: Swift array storage typically has count at +0x10.
        var count = arr.add(0x10).readU64();
        var positions = [];
        for (var i = 0; i < count.toNumber() && i < 64; i++) {
          var entry = arr.add(i * 0x28);
          // Try multiple offsets to find the position
          try {
            var off_at_20 = entry.add(0x20).readU64();
            var ct_at_28 = entry.add(0x28).readU16();
            var sz_at_2c = entry.add(0x2c).readU32();
            positions.push({i: i, off: off_at_20.toNumber(), ct: "0x" + ct_at_28.toString(16), sz: sz_at_2c});
          } catch (e) {}
        }
        console.log(JSON.stringify({
          msg: "scan_leave",
          ct: "0x" + this.ct.toString(16),
          count: count.toNumber(),
          entries: positions
        }));
      } catch (e) {
        console.log(JSON.stringify({msg:"scan_leave_err", err: e.toString()}));
      }
    }
  });
  console.log(JSON.stringify({msg:"hook_ok", target:"FUN_100175f6c", at:blockScan.toString()}));
} catch (e) {
  console.log(JSON.stringify({msg:"hook_err", target:"FUN_100175f6c", err: e.toString()}));
}

// Hook emit sites for correlation
EMITS.forEach(function(p){
  var off = p[0], feat = p[1];
  var addr = base.add(off);
  try {
    Interceptor.attach(addr, function() {
      console.log(JSON.stringify({msg:"emit", f: feat}));
    });
  } catch(e) {
    console.log(JSON.stringify({msg:"emit_hook_err", f: feat, err: e.toString()}));
  }
});

console.log(JSON.stringify({msg:"ready"}));
