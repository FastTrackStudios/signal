var pt = Process.enumerateModules().filter(function(m){return m.name.indexOf("PT Reaper")>=0;})[0];
console.log(JSON.stringify({msg:"base", b: pt.base.toString()}));
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
var fn = findExport("libsystem_c.dylib", "_platform_memmove");
var bucket = {};
if (fn) {
  Interceptor.attach(fn, {
    onEnter: function(args) {
      var len = args[2].toInt32();
      var key = len <= 16 ? '0-16' : (len <= 256 ? '17-256' : (len <= 4096 ? '257-4K' : (len <= 65536 ? '4K-64K' : '>64K')));
      bucket[key] = (bucket[key]||0) + 1;
    }
  });
  console.log(JSON.stringify({msg:"hook_ok"}));
}
// Report on script end via timer
setTimeout(function(){ console.log(JSON.stringify({msg:"summary", bucket: bucket})); }, 8000);
