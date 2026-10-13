// Trace `memcpy`/`memmove` reads from the decrypted PTX buffer.
//
// Approach: hook `read()`/`pread()` to find when the PTX file is
// loaded into memory. Capture the buffer range. Then hook `memcpy`
// and log every call whose source lies inside that range.
//
// This catches multi-byte numeric reads (i32 vol, u64 positions,
// UIDs etc.) that the single-byte `Data.subscript` hook misses.

var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
var base = pt.base;
console.log(JSON.stringify({msg:"module_base", base: base.toString()}));

// Diagnostic: list libsystem_c exports related to memcpy/memmove
try {
  var m = Process.getModuleByName("libsystem_c.dylib");
  if (m) {
    var exp = m.enumerateExports();
    exp.forEach(function(e){
      if (e.name.indexOf("mem") >= 0 || e.name.indexOf("read") >= 0) {
        console.log(JSON.stringify({msg:"export", mod:"libsystem_c", name:e.name, addr:e.address.toString()}));
      }
    });
  }
} catch (e) { console.log(JSON.stringify({msg:"err", err: e.toString()})); }
try {
  var mp = Process.getModuleByName("libsystem_platform.dylib");
  if (mp) {
    mp.enumerateExports().forEach(function(e){
      if (e.name.indexOf("mem") >= 0 || e.name.indexOf("read") >= 0) {
        console.log(JSON.stringify({msg:"export", mod:"libsystem_platform", name:e.name, addr:e.address.toString()}));
      }
    });
  }
} catch (e) {}

// Track candidate buffers: when a large `read` returns, remember
// the destination buffer + length. The PTX file is typically a few
// MB so we filter to ranges >= 100 KB and <= 50 MB.
var buffers = [];   // {addr: NativePointer, size: int}

function withinBuffer(p) {
  for (var i = 0; i < buffers.length; i++) {
    var b = buffers[i];
    if (p.compare(b.addr) >= 0 && p.compare(b.addr.add(b.size)) < 0) {
      return {idx: i, offset: p.sub(b.addr).toInt32()};
    }
  }
  return null;
}

// Hook `read` and `pread`. Try several module names.
function tryHookFn(modules, name, attach) {
  for (var i = 0; i < modules.length; i++) {
    var fn = null;
    try { fn = Module.findExportByName(modules[i], name); } catch (e) {}
    if (fn) {
      try {
        Interceptor.attach(fn, attach);
        console.log(JSON.stringify({msg:"hook_ok", mod: modules[i], name: name, at: fn.toString()}));
        return true;
      } catch (e) {
        console.log(JSON.stringify({msg:"hook_err", name: name, err: e.toString()}));
      }
    }
  }
  console.log(JSON.stringify({msg:"hook_unresolved", name: name}));
  return false;
}

// Fallback: register large memmoves where the DEST is a fresh
// allocation, since file loads typically copy from kernel buffer
// to a Data/NSData backing store via memmove.
//
// We use the destination as the candidate buffer for subsequent
// reads.
var primaryBufferCaptured = false;
function maybeRegisterBuffer(dst, len) {
  if (primaryBufferCaptured) return;
  if (len < 100000 || len > 50 * 1024 * 1024) return;
  buffers.push({addr: dst, size: len});
  primaryBufferCaptured = true;
  console.log(JSON.stringify({msg:"buffer_register", addr: dst.toString(), size: len}));
}

// Hook memcpy/memmove. On Darwin arm64 the real implementations are
// `_platform_memmove` etc. in libsystem_c.dylib. Use module-scoped
// export enumeration to find them by direct name match.
function findExport(modName, exportName) {
  try {
    var m = Process.getModuleByName(modName);
    if (!m) return null;
    var exps = m.enumerateExports();
    for (var i = 0; i < exps.length; i++) {
      if (exps[i].name === exportName) return exps[i].address;
    }
  } catch (e) {}
  return null;
}

["_platform_memmove", "_platform_memcpy"].forEach(function(name){
  var fn = findExport("libsystem_c.dylib", name);
  if (!fn) fn = findExport("libsystem_platform.dylib", name);
  if (!fn) {
    console.log(JSON.stringify({msg:"hook_unresolved", name:name}));
    return;
  }
  try {
    Interceptor.attach(fn, {
      onEnter: function(args) {
        this.dst = args[0];
        this.src = args[1];
        this.len = args[2].toInt32();
      },
      onLeave: function() {
        var src = this.src, len = this.len, dst = this.dst;
        if (len >= 100000) {
          // Likely a file/buffer load
          maybeRegisterBuffer(dst, len);
          return;
        }
        if (len < 1 || len > 1024) return;
        var hit = withinBuffer(src);
        if (hit !== null) {
          var preview = [];
          try {
            for (var i = 0; i < Math.min(len, 16); i++) {
              preview.push(src.add(i).readU8());
            }
          } catch (e) {}
          console.log(JSON.stringify({
            msg:"cpy", fn:name, off:hit.offset, len:len, preview:preview
          }));
        }
      }
    });
    console.log(JSON.stringify({msg:"hook_ok", name:name, at:fn.toString()}));
  } catch (e) {
    console.log(JSON.stringify({msg:"hook_err", name:name, err:e.toString()}));
  }
});

console.log(JSON.stringify({msg:"ready"}));
