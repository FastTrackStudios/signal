// Minimal CLAP loader: dlopen a bundle's binary, run its entry point, and
// ask the plugin factory how many plugins it exposes. Proves the bundle is
// actually loadable by a host — `nm` showing a clap_entry symbol does not,
// since a missing dependency or a failing init only surfaces at load time.
//
// Argument is the Mach-O/ELF itself, not the bundle directory: on macOS a
// .clap is a directory and dlopen cannot open one.
// Built and run by `just plugins-verify`.
#include <dlfcn.h>
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct clap_plugin_entry {
    struct { uint32_t major, minor, revision; } clap_version;
    bool (*init)(const char *plugin_path);
    void (*deinit)(void);
    const void *(*get_factory)(const char *factory_id);
} clap_plugin_entry_t;

typedef struct clap_plugin_factory {
    uint32_t (*get_plugin_count)(const struct clap_plugin_factory *);
    const void *(*get_plugin_descriptor)(const struct clap_plugin_factory *, uint32_t);
    const void *(*create_plugin)(const struct clap_plugin_factory *, const void *, const char *);
} clap_plugin_factory_t;

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("FAIL dlopen: %s\n", dlerror()); return 1; }
    clap_plugin_entry_t *e = (clap_plugin_entry_t *)dlsym(h, "clap_entry");
    if (!e) { printf("FAIL no clap_entry\n"); return 1; }
    if (!e->init(argv[1])) { printf("FAIL entry->init returned false\n"); return 1; }
    const clap_plugin_factory_t *f =
        (const clap_plugin_factory_t *)e->get_factory("clap.plugin-factory");
    if (!f) { printf("FAIL no plugin factory\n"); return 1; }
    uint32_t n = f->get_plugin_count(f);
    printf("OK  plugins=%u\n", n);
    e->deinit();
    return 0;
}
