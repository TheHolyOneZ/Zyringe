/*
 * Zyringe helper payload — libzyringe.so
 * -------------------------------------------------------------------------
 * Author:  TheHolyOneZ   (https://zlogic.eu/mods/)
 *
 * Runs INSIDE a target Unity/Mono game. Two entry paths, one binary:
 *
 *   ATTACH mode (primary, SMI-grade):
 *     The privileged `zyringe-inject` ptrace-loads this .so and remote-calls the
 *     exported `zyringe_run(zy_req*)`. That request lives entirely in target
 *     memory and carries the DLL *bytes* (not a path). zyringe_run spawns a
 *     fresh thread that waits for a `go` flag, then drives the Mono embedding
 *     API to load the assembly FROM MEMORY (mono_image_open_from_data) and
 *     invoke the entry point — like SharpMonoInjector, but on a clean dedicated
 *     thread so it never corrupts a thread caught mid-operation. Result and
 *     exception text are written back into the request for the injector to read
 *     via process_vm_readv. No file ever touches disk.
 *
 *   LAUNCH mode (fallback):
 *     Game started with LD_PRELOAD=libzyringe.so and ZYRINGE_CONFIG=<json>.
 *     The ELF constructor reads that config and loads the assembly from a path.
 *
 * Self-contained: links only libc. Every Mono function is an opaque pointer
 * resolved at runtime via dlsym, so one build works across Mono flavors.
 * Build: gcc -shared -fPIC -O2 -o libzyringe.so zyringe_helper.c -ldl -lpthread
 */

#define _GNU_SOURCE
#include <dlfcn.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define ZY_EXPORT __attribute__((visibility("default")))

/* ---- request shared with zyringe-inject (identical layout in Rust) ------ */
#define ZY_MAGIC 0x5A595251u /* "ZYRQ" */
enum { ZY_PENDING = 0, ZY_OK = 1, ZY_ERR = 2 };

typedef struct {
    uint32_t magic;       /* 0  */
    uint32_t go;          /* 4  set to 1 by injector once the process is resumed */
    uint32_t status;      /* 8  ZY_PENDING/OK/ERR, written by us */
    uint32_t pad;         /* 12 */
    uint64_t dll_ptr;     /* 16 absolute addr of DLL bytes in this process */
    uint64_t dll_len;     /* 24 */
    uint64_t ns_ptr;      /* 32 */
    uint64_t class_ptr;   /* 40 */
    uint64_t method_ptr;  /* 48 */
    uint64_t mono_so_ptr; /* 56 absolute addr of mono .so path cstr ("" = probe) */
    uint64_t error_ptr;   /* 64 absolute addr of error buffer */
    uint64_t error_cap;   /* 72 */
    uint64_t dll_path_ptr;/* 80 absolute addr of the DLL's real path cstr */
    uint32_t argc;        /* 88 number of string arguments */
    uint32_t pad2;        /* 92 */
    uint64_t args_ptr;    /* 96 addr of argc NUL-terminated arg strings, packed */
    uint64_t ret_ptr;     /* 104 buffer for the return value's ToString() */
    uint64_t ret_cap;     /* 112 */
    uint64_t total_size;  /* 120 mmap size, so the worker can munmap on exit */
} zy_req;                 /* size 128 */

/* ---- tunables ---------------------------------------------------------- */
#define ZY_DOMAIN_TIMEOUT_MS 30000
#define ZY_MONO_TIMEOUT_MS   30000
#define ZY_POLL_US           5000
#define ZY_PATH_MAX          4096

/* ---- opaque Mono types ------------------------------------------------- */
typedef void MonoDomain;
typedef void MonoAssembly;
typedef void MonoImage;
typedef void MonoClass;
typedef void MonoMethod;
typedef void MonoObject;
typedef void MonoString;
typedef void MonoThread;

/* ---- Mono function pointer typedefs ------------------------------------ */
typedef MonoDomain *(*PFN_get_root_domain)(void);
typedef MonoThread *(*PFN_thread_attach)(MonoDomain *);
typedef void (*PFN_thread_detach)(MonoThread *);
typedef MonoImage *(*PFN_image_open_from_data)(char *, uint32_t, int32_t, int *);
typedef MonoImage *(*PFN_image_open_from_data_with_name)(char *, uint32_t, int32_t, int *,
                                                        int32_t, const char *);
typedef MonoAssembly *(*PFN_assembly_load_from_full)(MonoImage *, const char *, int *, int32_t);
typedef MonoImage *(*PFN_assembly_get_image)(MonoAssembly *);
typedef MonoAssembly *(*PFN_domain_assembly_open)(MonoDomain *, const char *);
typedef MonoClass *(*PFN_class_from_name)(MonoImage *, const char *, const char *);
typedef MonoMethod *(*PFN_get_method_from_name)(MonoClass *, const char *, int);
typedef MonoObject *(*PFN_runtime_invoke)(MonoMethod *, void *, void **, MonoObject **);
typedef MonoClass *(*PFN_object_get_class)(MonoObject *);
typedef const char *(*PFN_class_get_name)(MonoClass *);
typedef const char *(*PFN_image_get_name)(MonoImage *);
typedef MonoString *(*PFN_object_to_string)(MonoObject *, MonoObject **);
typedef char *(*PFN_string_to_utf8)(MonoString *);
typedef MonoString *(*PFN_string_new)(MonoDomain *, const char *);

static struct {
    PFN_get_root_domain get_root_domain;
    PFN_thread_attach thread_attach;
    PFN_thread_detach thread_detach;
    PFN_image_open_from_data image_open_from_data;
    PFN_image_open_from_data_with_name image_open_from_data_with_name; /* optional */
    PFN_assembly_load_from_full assembly_load_from_full;
    PFN_assembly_get_image assembly_get_image;
    PFN_domain_assembly_open domain_assembly_open;
    PFN_class_from_name class_from_name;
    PFN_get_method_from_name get_method_from_name;
    PFN_runtime_invoke runtime_invoke;
    PFN_object_get_class object_get_class;
    PFN_class_get_name class_get_name;
    PFN_image_get_name image_get_name;
    PFN_object_to_string object_to_string; /* optional */
    PFN_string_to_utf8 string_to_utf8;     /* optional */
    PFN_string_new string_new;             /* optional (needed for args) */
} M;

/* ---- logging ----------------------------------------------------------- */
static FILE *g_log = NULL;

static void zylog(const char *fmt, ...) {
    char ts[16];
    time_t t = time(NULL);
    struct tm tm;
    localtime_r(&t, &tm);
    strftime(ts, sizeof(ts), "%H:%M:%S", &tm);
    va_list ap;
    if (g_log) {
        fprintf(g_log, "[%s] ", ts);
        va_start(ap, fmt);
        vfprintf(g_log, fmt, ap);
        va_end(ap);
        fputc('\n', g_log);
        fflush(g_log);
    }
    fprintf(stderr, "[zyringe %s] ", ts);
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    va_end(ap);
    fputc('\n', stderr);
}

static void open_log(void) {
    if (g_log) return;
    char p[ZY_PATH_MAX];
    snprintf(p, sizeof(p), "/tmp/.zyringe/helper-%d.log", (int)getpid());
    g_log = fopen(p, "a");
}

/* ---- Mono symbol resolution -------------------------------------------- */
/* returns 1 if all *required* symbols resolved */
static int resolve_mono(void *h) {
#define REQ(field, name)                                                       \
    M.field = (void *)dlsym(h, name);                                          \
    if (!M.field) {                                                            \
        zylog("missing required symbol %s", name);                             \
        return 0;                                                             \
    }
#define OPT(field, name) M.field = (void *)dlsym(h, name);

    REQ(get_root_domain, "mono_get_root_domain")
    REQ(thread_attach, "mono_thread_attach")
    REQ(class_from_name, "mono_class_from_name")
    REQ(get_method_from_name, "mono_class_get_method_from_name")
    REQ(runtime_invoke, "mono_runtime_invoke")
    REQ(assembly_get_image, "mono_assembly_get_image")
    REQ(object_get_class, "mono_object_get_class")
    REQ(class_get_name, "mono_class_get_name")
    REQ(image_get_name, "mono_image_get_name")
    OPT(thread_detach, "mono_thread_detach")
    OPT(image_open_from_data, "mono_image_open_from_data")
    OPT(image_open_from_data_with_name, "mono_image_open_from_data_with_name")
    OPT(assembly_load_from_full, "mono_assembly_load_from_full")
    OPT(domain_assembly_open, "mono_domain_assembly_open")
    OPT(object_to_string, "mono_object_to_string")
    OPT(string_to_utf8, "mono_string_to_utf8")
    OPT(string_new, "mono_string_new")
#undef REQ
#undef OPT
    return 1;
}

/* Obtain a handle whose symbol table exposes the Mono API. Prefer the exact
 * .so the injector located; fall back to the global scope. */
static void *acquire_mono(const char *mono_so) {
    int elapsed = 0;
    while (elapsed < ZY_MONO_TIMEOUT_MS) {
        if (mono_so && mono_so[0]) {
            void *h = dlopen(mono_so, RTLD_NOW | RTLD_NOLOAD);
            if (h && dlsym(h, "mono_get_root_domain")) return h;
            if (h) dlclose(h);
        }
        void *g = dlopen(NULL, RTLD_NOW);
        if (g && dlsym(g, "mono_get_root_domain")) return g;
        if (g) dlclose(g);
        usleep(ZY_POLL_US);
        elapsed += ZY_POLL_US / 1000;
    }
    return NULL;
}

static MonoDomain *wait_for_domain(void) {
    int elapsed = 0;
    while (elapsed < ZY_DOMAIN_TIMEOUT_MS) {
        MonoDomain *d = M.get_root_domain();
        if (d) return d;
        usleep(ZY_POLL_US);
        elapsed += ZY_POLL_US / 1000;
    }
    return NULL;
}

/* Describe a managed exception into `out` (best-effort message + class). */
static void describe_exception(MonoObject *exc, char *out, size_t cap) {
    const char *cls = "Exception";
    MonoClass *ec = M.object_get_class(exc);
    if (ec) cls = M.class_get_name(ec);

    char *msg = NULL;
    if (M.object_to_string && M.string_to_utf8) {
        MonoObject *strexc = NULL;
        MonoString *s = M.object_to_string(exc, &strexc);
        if (s && !strexc) msg = M.string_to_utf8(s);
    }
    if (msg) {
        snprintf(out, cap, "%s: %s", cls, msg);
    } else {
        snprintf(out, cap, "managed exception: %s", cls);
    }
}

/* Invoke a static method (0+ string args) and capture its return value's
 * ToString() into ret_buf. Arguments are marshalled as System.String. */
static int invoke_entry(MonoDomain *domain, MonoImage *image, const char *ns,
                        const char *cls, const char *method, char **argv,
                        uint32_t argc, char *err, size_t errcap, char *ret_buf,
                        size_t ret_cap) {
    MonoClass *klass = M.class_from_name(image, ns ? ns : "", cls);
    if (!klass) {
        snprintf(err, errcap, "class not found: %s.%s", ns ? ns : "", cls);
        return 0;
    }
    MonoMethod *m = M.get_method_from_name(klass, method, (int)argc);
    if (!m) {
        snprintf(err, errcap, "method not found: %s.%s.%s (%u arg%s)", ns ? ns : "",
                 cls, method, argc, argc == 1 ? "" : "s");
        return 0;
    }

    void **params = NULL;
    if (argc > 0) {
        if (!M.string_new) {
            snprintf(err, errcap, "mono_string_new unavailable; cannot pass arguments");
            return 0;
        }
        params = (void **)calloc(argc, sizeof(void *));
        for (uint32_t i = 0; i < argc; i++) {
            params[i] = M.string_new(domain, argv[i] ? argv[i] : "");
        }
    }

    MonoObject *exc = NULL;
    MonoObject *ret = M.runtime_invoke(m, NULL, params, &exc);
    free(params);
    if (exc) {
        describe_exception(exc, err, errcap);
        return 0;
    }

    /* best-effort: report the return value's ToString() */
    if (ret && ret_buf && ret_cap && M.object_to_string && M.string_to_utf8) {
        MonoObject *e2 = NULL;
        MonoString *s = M.object_to_string(ret, &e2);
        if (s && !e2) {
            char *u = M.string_to_utf8(s);
            if (u) {
                snprintf(ret_buf, ret_cap, "%s", u);
            }
        }
    }
    zylog("invoked %s.%s.%s (%u args) OK", ns ? ns : "", cls, method, argc);
    return 1;
}

/* =======================================================================
 *  ATTACH mode — driven by the injector via zyringe_run()
 * ======================================================================= */

/* Publish the result, then (after the injector has had time to read it via
 * process_vm_readv) release the scratch page so we don't leak it in the target. */
static void zy_report(zy_req *req, int ok) {
    __atomic_store_n(&req->status, ok ? (uint32_t)ZY_OK : (uint32_t)ZY_ERR,
                     __ATOMIC_RELEASE);
    void *base = (void *)req;
    size_t sz = (size_t)req->total_size;
    sleep(3);
    if (base && sz) {
        munmap(base, sz);
    }
}

static void *attach_worker(void *arg) {
    zy_req *req = (zy_req *)arg;
    char *err = (char *)(uintptr_t)req->error_ptr;
    size_t errcap = (size_t)req->error_cap;
    char *ret_buf = (char *)(uintptr_t)req->ret_ptr;
    size_t ret_cap = (size_t)req->ret_cap;
    if (err && errcap) err[0] = '\0';
    if (ret_buf && ret_cap) ret_buf[0] = '\0';

    /* Wait until the injector has restored + detached the process, so the whole
     * game is running again before we touch Mono (avoids GC deadlock). */
    int waited = 0;
    while (!__atomic_load_n(&req->go, __ATOMIC_ACQUIRE)) {
        usleep(1000);
        if (++waited > 10000) { /* 10s safety */
            snprintf(err, errcap, "timed out waiting for go signal");
            zy_report(req, 0);
            return NULL;
        }
    }

    const char *mono_so = (const char *)(uintptr_t)req->mono_so_ptr;
    const char *ns = (const char *)(uintptr_t)req->ns_ptr;
    const char *cls = (const char *)(uintptr_t)req->class_ptr;
    const char *method = (const char *)(uintptr_t)req->method_ptr;
    const char *dll_path = (const char *)(uintptr_t)req->dll_path_ptr;

    void *h = acquire_mono(mono_so);
    if (!h || !resolve_mono(h)) {
        snprintf(err, errcap, "could not resolve Mono runtime in-process");
        zy_report(req, 0);
        return NULL;
    }
    if (!M.image_open_from_data || !M.assembly_load_from_full) {
        snprintf(err, errcap, "Mono lacks in-memory load API (image_open_from_data)");
        zy_report(req, 0);
        return NULL;
    }

    MonoDomain *domain = wait_for_domain();
    if (!domain) {
        snprintf(err, errcap, "timed out waiting for Mono root domain");
        zy_report(req, 0);
        return NULL;
    }

    /* Unpack the packed argument strings (arg0\0 arg1\0 …). */
    uint32_t argc = req->argc;
    char **argv = NULL;
    if (argc > 0 && req->args_ptr) {
        argv = (char **)calloc(argc, sizeof(char *));
        const char *p = (const char *)(uintptr_t)req->args_ptr;
        for (uint32_t i = 0; i < argc; i++) {
            argv[i] = (char *)p;
            p += strlen(p) + 1;
        }
    }

    MonoThread *thr = M.thread_attach(domain);

    /* Load the assembly straight from memory — no file on disk. Passing the real
     * DLL path as the image name makes Assembly.Location report that path. */
    int ok = 0;
    int st = 0;
    const char *img_name = (dll_path && dll_path[0]) ? dll_path : "zyringe-mod.dll";
    MonoImage *image = M.image_open_from_data_with_name
        ? M.image_open_from_data_with_name((char *)(uintptr_t)req->dll_ptr,
                                           (uint32_t)req->dll_len, 1, &st, 0, img_name)
        : M.image_open_from_data((char *)(uintptr_t)req->dll_ptr,
                                 (uint32_t)req->dll_len, 1, &st);
    if (!image || st != 0) {
        snprintf(err, errcap, "mono_image_open_from_data failed (status %d)", st);
    } else {
        st = 0;
        MonoAssembly *asmb = M.assembly_load_from_full(image, "", &st, 0);
        if (!asmb || st != 0) {
            snprintf(err, errcap, "mono_assembly_load_from_full failed (status %d)", st);
        } else {
            MonoImage *real = M.assembly_get_image(asmb);
            if (!real) {
                snprintf(err, errcap, "mono_assembly_get_image returned NULL");
            } else {
                zylog("loaded assembly from memory: %s", M.image_get_name(real));
                ok = invoke_entry(domain, real, ns, cls, method, argv, argc, err,
                                  errcap, ret_buf, ret_cap);
            }
        }
    }

    free(argv);
    if (thr && M.thread_detach) M.thread_detach(thr);
    if (ok) {
        zylog("attach injection succeeded");
    } else {
        zylog("attach injection failed: %s", err);
    }
    zy_report(req, ok);
    return NULL;
}

/* Exported entry point the injector remote-calls. Validates the request,
 * spawns the worker, and returns immediately. */
ZY_EXPORT int zyringe_run(void *request) {
    open_log();
    zy_req *req = (zy_req *)request;
    if (!req || req->magic != ZY_MAGIC) {
        zylog("zyringe_run: bad request");
        return -1;
    }
    zylog("zyringe_run: request accepted (dll_len=%llu)",
          (unsigned long long)req->dll_len);

    pthread_t t;
    pthread_attr_t a;
    pthread_attr_init(&a);
    pthread_attr_setdetachstate(&a, PTHREAD_CREATE_DETACHED);
    int rc = pthread_create(&t, &a, attach_worker, req);
    pthread_attr_destroy(&a);
    if (rc != 0) {
        zylog("pthread_create failed: %d", rc);
        return -1;
    }
    return 0;
}

/* =======================================================================
 *  LAUNCH mode — LD_PRELOAD + ZYRINGE_CONFIG (disk path)
 * ======================================================================= */

/* minimal JSON string-value extractor (flat, machine-generated config) */
static int json_get(const char *buf, const char *key, char *out, size_t outsz) {
    char needle[128];
    snprintf(needle, sizeof(needle), "\"%s\"", key);
    const char *p = strstr(buf, needle);
    if (!p) return 0;
    p += strlen(needle);
    while (*p && *p != ':') p++;
    if (*p != ':') return 0;
    p++;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    if (*p != '"') return 0;
    p++;
    size_t i = 0;
    while (*p && *p != '"' && i + 1 < outsz) {
        if (*p == '\\' && p[1]) p++;
        out[i++] = *p++;
    }
    out[i] = '\0';
    return 1;
}

static void *preload_worker(void *arg) {
    char *cfgpath = (char *)arg;
    char buf[8192];
    FILE *f = fopen(cfgpath, "rb");
    free(cfgpath);
    if (!f) {
        zylog("preload: cannot open config");
        return NULL;
    }
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = '\0';

    char dll[ZY_PATH_MAX] = {0}, ns[512] = {0}, cls[512] = {0}, method[512] = {0},
         mono_so[ZY_PATH_MAX] = {0};
    if (!json_get(buf, "dll", dll, sizeof(dll)) ||
        !json_get(buf, "class", cls, sizeof(cls)) ||
        !json_get(buf, "method", method, sizeof(method))) {
        zylog("preload: incomplete config");
        return NULL;
    }
    json_get(buf, "namespace", ns, sizeof(ns));
    json_get(buf, "mono_so", mono_so, sizeof(mono_so));

    void *h = acquire_mono(mono_so);
    if (!h || !resolve_mono(h) || !M.domain_assembly_open) {
        zylog("preload: could not resolve Mono");
        return NULL;
    }
    MonoDomain *domain = wait_for_domain();
    if (!domain) {
        zylog("preload: no root domain");
        return NULL;
    }
    /* let the scripting bridge settle before attaching */
    usleep(1500 * 1000);
    MonoThread *thr = M.thread_attach(domain);

    MonoAssembly *asmb = M.domain_assembly_open(domain, dll);
    if (!asmb) {
        zylog("preload: failed to open %s", dll);
        if (thr && M.thread_detach) M.thread_detach(thr);
        return NULL;
    }
    MonoImage *image = M.assembly_get_image(asmb);
    char err[512] = {0};
    if (image)
        invoke_entry(domain, image, ns, cls, method, NULL, 0, err, sizeof(err), NULL, 0);
    if (err[0]) zylog("preload: %s", err);

    if (thr && M.thread_detach) M.thread_detach(thr);
    return NULL;
}

__attribute__((constructor)) static void zyringe_ctor(void) {
    open_log();
    const char *cfg = getenv("ZYRINGE_CONFIG");
    if (!cfg || !cfg[0]) {
        /* attach mode: nothing to do until zyringe_run() is called */
        zylog("libzyringe.so loaded into PID %d (attach mode)", (int)getpid());
        return;
    }
    zylog("libzyringe.so loaded into PID %d (LD_PRELOAD mode)", (int)getpid());
    char *dup = strdup(cfg);
    pthread_t t;
    pthread_attr_t a;
    pthread_attr_init(&a);
    pthread_attr_setdetachstate(&a, PTHREAD_CREATE_DETACHED);
    if (pthread_create(&t, &a, preload_worker, dup) != 0) free(dup);
    pthread_attr_destroy(&a);
}
