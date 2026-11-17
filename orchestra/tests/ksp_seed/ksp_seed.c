// ksp_seed.clap — makes Kontakt's KSP random() reproducible in a render.
//
// Kontakt's KSP random() draws from libc rand() (three calls per draw), which
// Kontakt seeds with srand(time(NULL)). Placed in a track's FX chain right
// before Kontakt, this plugin rebinds Kontakt's imports of rand() and srand()
// (its __got / symbol pointer entries, once per process) to wrappers over a
// Park–Miller state — the same generator as macOS libc. At the first block the
// transport plays it sets that state to the seed; from then on Kontakt's
// srand() calls are ignored, so the draws depend only on the seed, as KONTRA's
// (KONTRA_KSP_SEED) do.
//
// One state per instance (per track). Each instance's process() points a
// thread-local at its own state; REAPER runs a track's FX chain serially on
// one thread per block, so the Kontakt that follows on that thread draws from
// that track's state and nothing else. Several seeded Kontakt tracks in one
// project (a batch render, stems) then each draw exactly the sequence they
// draw rendered alone, whatever REAPER's track threads interleave. A rand()
// on a thread no instance has run on (none seen in Kontakt 8.7.1: every KSP
// draw is on the processing thread) draws from a shared fallback state.
//
// The seed: a track named "<anything>@<n>" (CLAP track-info) seeds with n;
// otherwise KSP_SEED. Neither: the plugin does nothing.
// KSP_SEED_LOG=<file>: every rand()/srand() call Kontakt makes after the
// rebind is logged with its caller (offset in the Kontakt image), the value
// returned, the track and the thread.
// MIDI and audio pass through (no ports: REAPER forwards both).
#include <clap/clap.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <mach-o/nlist.h>
#include <mach/mach.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

#define LOG_MAX_CALLS 200000UL

typedef struct {
    unsigned state;
    int pinned;
    unsigned long calls;
    char track[CLAP_NAME_SIZE];
} rng_slot;

typedef struct {
    clap_plugin_t plugin;
    const clap_host_t *host;
    int have_seed;
    unsigned seed;
    int was_playing;
    rng_slot slot;
} seed_plugin;

// The state Kontakt's rand() on this thread draws from: the instance whose
// process() ran last on this thread (the one before this track's Kontakt).
static __thread rng_slot *t_slot = NULL;
static rng_slot g_fallback = {.state = 1, .track = "(no track)"};
static pthread_mutex_t g_fallback_mu = PTHREAD_MUTEX_INITIALIZER;
static int g_any_pinned = 0;

static pthread_mutex_t g_rebind_mu = PTHREAD_MUTEX_INITIALIZER;
static int g_rebound = 0;
static uintptr_t g_kontakt_base = 0;

static pthread_mutex_t g_log_mu = PTHREAD_MUTEX_INITIALIZER;
static FILE *g_log = NULL;

static unsigned long thread_id(void) {
    uint64_t tid = 0;
    pthread_threadid_np(NULL, &tid);
    return (unsigned long)tid;
}

static int park_miller(rng_slot *s) {
    long ctx = (long)s->state;
    if (ctx == 0) ctx = 123459876;
    long hi = ctx / 127773, lo = ctx % 127773;
    long x = 16807 * lo - 2836 * hi;
    if (x < 0) x += 0x7fffffff;
    s->state = (unsigned)x;
    return (int)x;
}

static void log_call(const rng_slot *s, const char *what, unsigned long n, uintptr_t ra, long v) {
    if (!g_log || n >= LOG_MAX_CALLS) return;
    pthread_mutex_lock(&g_log_mu);
    fprintf(g_log, "%s %lu caller %#lx -> %ld track %s tid %lu\n", what, n,
            (unsigned long)(ra - g_kontakt_base), v, s->track, thread_id());
    pthread_mutex_unlock(&g_log_mu);
}

static int hook_rand(void) {
    uintptr_t ra = (uintptr_t)__builtin_return_address(0);
    rng_slot *s = t_slot;
    int v;
    if (s) {
        v = park_miller(s);
        log_call(s, "rand", s->calls++, ra, v);
    } else {
        pthread_mutex_lock(&g_fallback_mu);
        v = park_miller(&g_fallback);
        log_call(&g_fallback, "rand", g_fallback.calls++, ra, v);
        pthread_mutex_unlock(&g_fallback_mu);
    }
    return v;
}

static void hook_srand(unsigned seed) {
    uintptr_t ra = (uintptr_t)__builtin_return_address(0);
    rng_slot *s = t_slot;
    int ignored = s ? s->pinned : g_any_pinned;
    if (g_log) {
        pthread_mutex_lock(&g_log_mu);
        fprintf(g_log, "srand(%u) caller %#lx %s track %s tid %lu\n", seed,
                (unsigned long)(ra - g_kontakt_base), ignored ? "ignored" : "applied",
                s ? s->track : g_fallback.track, thread_id());
        pthread_mutex_unlock(&g_log_mu);
    }
    if (ignored) return;
    if (s) {
        s->state = seed;
    } else {
        pthread_mutex_lock(&g_fallback_mu);
        g_fallback.state = seed;
        pthread_mutex_unlock(&g_fallback_mu);
    }
}

// Rebind the symbol pointers named `_rand` / `_srand` in one image.
static int rebind_image(const struct mach_header_64 *mh, intptr_t slide) {
    const struct load_command *lc = (const void *)(mh + 1);
    const struct symtab_command *symtab = NULL;
    const struct dysymtab_command *dysym = NULL;
    const struct segment_command_64 *linkedit = NULL;
    for (uint32_t i = 0; i < mh->ncmds; i++) {
        if (lc->cmd == LC_SYMTAB) symtab = (const void *)lc;
        else if (lc->cmd == LC_DYSYMTAB) dysym = (const void *)lc;
        else if (lc->cmd == LC_SEGMENT_64 && strcmp(((const struct segment_command_64 *)lc)->segname, SEG_LINKEDIT) == 0)
            linkedit = (const void *)lc;
        lc = (const void *)((const char *)lc + lc->cmdsize);
    }
    if (!symtab || !dysym || !linkedit) return 0;
    uintptr_t le_base = (uintptr_t)slide + linkedit->vmaddr - linkedit->fileoff;
    const struct nlist_64 *syms = (const void *)(le_base + symtab->symoff);
    const char *strs = (const char *)(le_base + symtab->stroff);
    const uint32_t *indirect = (const void *)(le_base + dysym->indirectsymoff);
    int n = 0;
    lc = (const void *)(mh + 1);
    for (uint32_t i = 0; i < mh->ncmds; i++, lc = (const void *)((const char *)lc + lc->cmdsize)) {
        if (lc->cmd != LC_SEGMENT_64) continue;
        const struct segment_command_64 *seg = (const void *)lc;
        const struct section_64 *sect = (const void *)(seg + 1);
        for (uint32_t s = 0; s < seg->nsects; s++, sect++) {
            uint32_t type = sect->flags & SECTION_TYPE;
            if (type != S_LAZY_SYMBOL_POINTERS && type != S_NON_LAZY_SYMBOL_POINTERS) continue;
            void **ptrs = (void **)((uintptr_t)slide + sect->addr);
            uint32_t count = (uint32_t)(sect->size / sizeof(void *));
            for (uint32_t k = 0; k < count; k++) {
                uint32_t idx = indirect[sect->reserved1 + k];
                if (idx & (INDIRECT_SYMBOL_ABS | INDIRECT_SYMBOL_LOCAL)) continue;
                const char *name = strs + syms[idx].n_un.n_strx;
                void *to = NULL;
                if (strcmp(name, "_rand") == 0) to = (void *)hook_rand;
                else if (strcmp(name, "_srand") == 0) to = (void *)hook_srand;
                if (!to) continue;
                uintptr_t page = (uintptr_t)&ptrs[k] & ~(uintptr_t)(vm_page_size - 1);
                vm_protect(mach_task_self(), page, vm_page_size, 0, VM_PROT_READ | VM_PROT_WRITE | VM_PROT_COPY);
                ptrs[k] = to;
                n++;
            }
        }
    }
    return n;
}

// Once per process (every instance calls it; the first that finds Kontakt
// loaded rebinds it, the rest return at once).
static void rebind_kontakt_once(void) {
    pthread_mutex_lock(&g_rebind_mu);
    if (!g_rebound) {
        int total = 0;
        for (uint32_t i = 0; i < _dyld_image_count(); i++) {
            const char *name = _dyld_get_image_name(i);
            if (!name || !strstr(name, "Kontakt")) continue;
            const struct mach_header_64 *mh = (const void *)_dyld_get_image_header(i);
            intptr_t slide = _dyld_get_image_vmaddr_slide(i);
            int n = rebind_image(mh, slide);
            // caller offsets are relative to the image that imports rand()
            // (vmaddr 0, MH_BUNDLE)
            if (n > 0 && !g_kontakt_base) g_kontakt_base = (uintptr_t)slide;
            if (g_log) {
                pthread_mutex_lock(&g_log_mu);
                fprintf(g_log, "rebound %d pointers in %s (slide %#lx)\n", n, name, (unsigned long)slide);
                pthread_mutex_unlock(&g_log_mu);
            }
            total += n;
        }
        g_rebound = total > 0;
    }
    pthread_mutex_unlock(&g_rebind_mu);
}

static void read_track(seed_plugin *s) {
    s->slot.track[0] = 0;
    if (!s->host || !s->host->get_extension) return;
    const clap_host_track_info_t *ti = s->host->get_extension(s->host, CLAP_EXT_TRACK_INFO);
    if (!ti) ti = s->host->get_extension(s->host, CLAP_EXT_TRACK_INFO_COMPAT);
    clap_track_info_t info;
    memset(&info, 0, sizeof info);
    if (ti && ti->get && ti->get(s->host, &info) && (info.flags & CLAP_TRACK_INFO_HAS_TRACK_NAME))
        snprintf(s->slot.track, sizeof s->slot.track, "%s", info.name);
}

// The seed: "<name>@<n>" in the track name, else KSP_SEED.
static void read_seed(seed_plugin *s) {
    const char *at = strrchr(s->slot.track, '@');
    char *end = NULL;
    if (at && at[1]) {
        unsigned long v = strtoul(at + 1, &end, 10);
        if (end && *end == 0) {
            s->have_seed = 1;
            s->seed = (unsigned)v;
            return;
        }
    }
    const char *v = getenv("KSP_SEED");
    s->have_seed = v && *v;
    s->seed = s->have_seed ? (unsigned)strtoul(v, NULL, 10) : 0;
}

static bool p_init(const clap_plugin_t *p) {
    seed_plugin *s = p->plugin_data;
    read_track(s);
    return true;
}
static void p_destroy(const clap_plugin_t *p) {
    seed_plugin *s = p->plugin_data;
    if (t_slot == &s->slot) t_slot = NULL;
    free(s);
}
static bool p_activate(const clap_plugin_t *p, double sr, uint32_t minf, uint32_t maxf) {
    (void)sr; (void)minf; (void)maxf;
    seed_plugin *s = p->plugin_data;
    read_track(s);
    read_seed(s);
    s->was_playing = 0;
    const char *log = getenv("KSP_SEED_LOG");
    if (s->have_seed && log && *log) {
        pthread_mutex_lock(&g_log_mu);
        if (!g_log) {
            g_log = fopen(log, "w");
            if (g_log) setvbuf(g_log, NULL, _IOLBF, 0);
        }
        pthread_mutex_unlock(&g_log_mu);
    }
    return true;
}
static void p_deactivate(const clap_plugin_t *p) {
    (void)p;
    pthread_mutex_lock(&g_log_mu);
    if (g_log) fflush(g_log);
    pthread_mutex_unlock(&g_log_mu);
}
static bool p_start(const clap_plugin_t *p) { (void)p; return true; }
static void p_stop(const clap_plugin_t *p) { (void)p; }
static void p_reset(const clap_plugin_t *p) { (void)p; }

static clap_process_status p_process(const clap_plugin_t *p, const clap_process_t *proc) {
    seed_plugin *s = p->plugin_data;
    if (!s->have_seed) return CLAP_PROCESS_CONTINUE;
    int playing = proc->transport && (proc->transport->flags & CLAP_TRANSPORT_IS_PLAYING);
    if (playing && !s->was_playing) {
        rebind_kontakt_once();
        s->slot.state = s->seed;
        s->slot.pinned = 1;
        s->slot.calls = 0;
        g_any_pinned = 1;
        if (g_log) {
            pthread_mutex_lock(&g_log_mu);
            fprintf(g_log, "transport start: state = %u track %s tid %lu\n", s->seed, s->slot.track, thread_id());
            pthread_mutex_unlock(&g_log_mu);
        }
    }
    s->was_playing = playing;
    // this track's Kontakt runs next, on this thread
    t_slot = &s->slot;
    return CLAP_PROCESS_CONTINUE;
}

static void ti_changed(const clap_plugin_t *p) { read_track(p->plugin_data); }
static const clap_plugin_track_info_t track_info_ext = {.changed = ti_changed};

static const void *p_ext(const clap_plugin_t *p, const char *id) {
    (void)p;
    if (strcmp(id, CLAP_EXT_TRACK_INFO) == 0 || strcmp(id, CLAP_EXT_TRACK_INFO_COMPAT) == 0) return &track_info_ext;
    return NULL;
}
static void p_main(const clap_plugin_t *p) { (void)p; }

static const char *features[] = {CLAP_PLUGIN_FEATURE_AUDIO_EFFECT, CLAP_PLUGIN_FEATURE_UTILITY, NULL};
static const clap_plugin_descriptor_t desc = {
    .clap_version = CLAP_VERSION_INIT,
    .id = "com.fts.ksp-seed",
    .name = "KSP Seed",
    .vendor = "FTS",
    .url = "",
    .manual_url = "",
    .support_url = "",
    .version = "1.2.0",
    .description = "Kontakt's rand()/srand() pinned to a per-track seed at transport start, for reproducible renders",
    .features = features,
};

static const clap_plugin_t *create(const clap_plugin_factory_t *f, const clap_host_t *host, const char *id) {
    (void)f;
    if (strcmp(id, desc.id) != 0) return NULL;
    seed_plugin *s = calloc(1, sizeof *s);
    s->host = host;
    s->slot.state = 1;
    s->plugin = (clap_plugin_t){
        .desc = &desc, .plugin_data = s, .init = p_init, .destroy = p_destroy,
        .activate = p_activate, .deactivate = p_deactivate, .start_processing = p_start,
        .stop_processing = p_stop, .reset = p_reset, .process = p_process,
        .get_extension = p_ext, .on_main_thread = p_main,
    };
    return &s->plugin;
}
static uint32_t count(const clap_plugin_factory_t *f) { (void)f; return 1; }
static const clap_plugin_descriptor_t *get_desc(const clap_plugin_factory_t *f, uint32_t i) {
    (void)f; return i == 0 ? &desc : NULL;
}
static const clap_plugin_factory_t factory = {.get_plugin_count = count, .get_plugin_descriptor = get_desc, .create_plugin = create};
static bool e_init(const char *path) { (void)path; return true; }
static void e_deinit(void) {
    pthread_mutex_lock(&g_log_mu);
    if (g_log) fclose(g_log);
    g_log = NULL;
    pthread_mutex_unlock(&g_log_mu);
}
static const void *e_factory(const char *id) {
    return strcmp(id, CLAP_PLUGIN_FACTORY_ID) == 0 ? &factory : NULL;
}
CLAP_EXPORT const clap_plugin_entry_t clap_entry = {
    .clap_version = CLAP_VERSION_INIT, .init = e_init, .deinit = e_deinit, .get_factory = e_factory,
};
