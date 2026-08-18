// Minimal VST3 loader: dlopen a bundle's binary, run its module entry, and
// ask for the plugin factory's class count through the IPluginFactory
// vtable. Same purpose as clap_load.c — proving the thing actually loads.
// Built and run by `just plugins-verify`.
#include <dlfcn.h>
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct IPluginFactoryVtbl {
    int32_t (*queryInterface)(void *, const char *, void **);
    uint32_t (*addRef)(void *);
    uint32_t (*release)(void *);
    int32_t (*getFactoryInfo)(void *, void *);
    int32_t (*countClasses)(void *);
    int32_t (*getClassInfo)(void *, int32_t, void *);
    int32_t (*createInstance)(void *, const char *, const char *, void **);
} IPluginFactoryVtbl;

typedef struct { IPluginFactoryVtbl *vtbl; } IPluginFactory;

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("FAIL dlopen: %s\n", dlerror()); return 1; }
    // VST3 names its module entry per platform: bundleEntry on macOS,
    // ModuleEntry on Linux, InitDll on Windows.
    bool (*entry)(void *) = (bool (*)(void *))dlsym(h, "bundleEntry");
    if (!entry) entry = (bool (*)(void *))dlsym(h, "ModuleEntry");
    IPluginFactory *(*getf)(void) = (IPluginFactory *(*)(void))dlsym(h, "GetPluginFactory");
    if (!entry || !getf) { printf("FAIL missing bundleEntry|ModuleEntry / GetPluginFactory\n"); return 1; }
    if (!entry(h)) { printf("FAIL module entry returned false\n"); return 1; }
    IPluginFactory *f = getf();
    if (!f) { printf("FAIL GetPluginFactory returned NULL\n"); return 1; }
    printf("OK  classes=%d\n", f->vtbl->countClasses(f));
    return 0;
}
