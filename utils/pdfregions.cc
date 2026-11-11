//========================================================================
//
// pdfregions.cc
//
// Render regions of PDF pages (figures, charts) as RGB pixels or SVG.
//
// This file is licensed under the GPLv2 or later
//
// Copyright (C) 2026 DocSlurp
//
//========================================================================

// Implements docslurp/extraction/PROTOCOL.md. Jobs come on stdin, one per
// line; records go to stdout in job order. The tool writes no files, decides
// nothing about what to extract, and never puts document data on stderr:
// every diagnostic is a fixed string. Limits (address space, pixels per crop,
// wall clock) are enforced here so a hostile document fails fast and cleanly.

#include "config.h"
#include <poppler-config.h>
#include <algorithm>
#include <cerrno>
#include <climits>
#include <cmath>
#include <csignal>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <memory>
#include <new>
#include <optional>
#include <string>
#include <vector>
#ifndef _WIN32
#    include <sys/resource.h>
#    include <sys/uio.h>
#    include <unistd.h>
#endif
#ifdef __linux__
#    include <cstddef>
#    include <linux/audit.h>
#    include <linux/filter.h>
#    include <linux/landlock.h>
#    include <linux/seccomp.h>
#    include <sys/prctl.h>
#    include <sys/syscall.h>
#    include <sys/socket.h>
#    include <netinet/in.h>
#endif
#if WITH_FONTCONFIGURATION_FONTCONFIG
#    include <fontconfig/fontconfig.h>
#endif
#include <cairo.h>
#include <cairo-svg.h>
#include "parseargs.h"
#include "goo/GooString.h"
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "Page.h"
#include "Error.h"
#include "ErrorCodes.h"
#include "SplashOutputDev.h"
#include "splash/SplashBitmap.h"
#include "CairoOutputDev.h"
#include "CairoFontEngine.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include "Win32Console.h"

enum ExitCode
{
    exitOk = 0,
    exitOpenError = 1,
    exitUsage = 2,
    exitLimit = 3,
    exitWriteError = 4,
    exitUnconfined = 5,
};

static constexpr size_t maxJobs = 5000;
static constexpr size_t maxLineLength = 256;

static char ownerPassword[128] = "\001";
static char userPassword[128] = "\001";
static char maxMemoryArg[32] = "2048";
static char maxPixelsArg[32] = "16777216";
static char timeoutArg[32] = "120";
static bool printVersion = false;
static bool printHelp = false;
static char confineArg[16] = "auto";
static char maxSvgBytesArg[32] = "33554432";
static char maxOutputBytesArg[32] = "536870912";
static bool noImages = false;
static char confineCheckPath[4096] = "";

static const ArgDesc argDesc[] = { { .arg = "-opw", .kind = argString, .val = ownerPassword, .size = sizeof(ownerPassword), .usage = "owner password (for encrypted files)" },
                                   { .arg = "-upw", .kind = argString, .val = userPassword, .size = sizeof(userPassword), .usage = "user password (for encrypted files)" },
                                   { .arg = "-max-memory", .kind = argString, .val = maxMemoryArg, .size = sizeof(maxMemoryArg), .usage = "address-space limit in MiB, 0 for none (default 2048)" },
                                   { .arg = "-max-pixels", .kind = argString, .val = maxPixelsArg, .size = sizeof(maxPixelsArg), .usage = "largest raster crop in pixels (default 16777216)" },
                                   { .arg = "-timeout", .kind = argString, .val = timeoutArg, .size = sizeof(timeoutArg), .usage = "wall-clock limit for the whole run in seconds, 0 for none (default 120)" },
                                   { .arg = "-max-svg-bytes", .kind = argString, .val = maxSvgBytesArg, .size = sizeof(maxSvgBytesArg), .usage = "largest SVG record; larger is err:svgsize (default 33554432)" },
                                   { .arg = "-max-output-bytes", .kind = argString, .val = maxOutputBytesArg, .size = sizeof(maxOutputBytesArg), .usage = "total stdout bytes; a record past it exits 3 (default 536870912)" },
                                   { .arg = "-no-images", .kind = argFlag, .val = &noImages, .size = 0, .usage = "draw no images (vector-only renders; image data is never decoded)" },
                                   { .arg = "-confine-check", .kind = argString, .val = confineCheckPath, .size = sizeof(confineCheckPath), .usage = "test aid: after confining, try to create this file, run a program and open a socket; report on stderr" },
                                   { .arg = "-confine", .kind = argString, .val = confineArg, .size = sizeof(confineArg), .usage = "no-write sandbox after opening the PDF: require, auto or off (default auto)" },
                                   { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
                                   { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   {} };

//------------------------------------------------------------------------
// diagnostics and limits
//------------------------------------------------------------------------

static int diagnosticsFd = 2;

static void writeDiagnostic(const char *msg)
{
    char line[160];
    const int n = snprintf(line, sizeof(line), "pdfregions: %s\n", msg);
    if (n > 0) {
#ifndef _WIN32
        ssize_t ignored = write(diagnosticsFd, line, std::min(static_cast<size_t>(n), sizeof(line) - 1));
        (void)ignored;
#else
        fputs(line, stderr);
#endif
    }
}

static int fail(int code, const char *msg)
{
    writeDiagnostic(msg);
    return code;
}

// Poppler's own messages can quote document strings, so they are dropped.
static void quietErrorCallback(ErrorCategory /*category*/, Goffset /*pos*/, const char * /*msg*/) { }

// Some Poppler and cairo paths write to stderr directly, so on POSIX fd 2 is
// pointed at /dev/null and this tool's fixed messages go to a saved copy.
static void silenceStderr()
{
#ifndef _WIN32
    const int saved = dup(2);
    const int devNull = open("/dev/null", O_WRONLY);
    if (saved >= 0 && devNull >= 0 && dup2(devNull, 2) >= 0) {
        diagnosticsFd = saved;
    }
    if (devNull >= 0) {
        close(devNull);
    }
#endif
}

static void limitMemory(long long mebibytes)
{
#ifndef _WIN32
    if (mebibytes > 0) {
        const rlim_t bytes = static_cast<rlim_t>(mebibytes) * 1024 * 1024;
        struct rlimit current;
        if (getrlimit(RLIMIT_AS, &current) == 0 && (current.rlim_max == RLIM_INFINITY || bytes < current.rlim_max)) {
            const struct rlimit wanted = { bytes, current.rlim_max };
            setrlimit(RLIMIT_AS, &wanted);
        }
    }
#else
    (void)mebibytes;
#endif
}

static void outOfMemory()
{
    writeDiagnostic("out of memory");
    _Exit(exitLimit);
}

#ifndef _WIN32
// When the deadline passes between records, the run stops at once and every
// record on stdout is complete. When it passes while a record is being
// written, that record gets 2 more seconds to finish (then the run stops
// after it); a write still blocked after that is cut short and the run exits,
// leaving a truncated last record that readers must drop.
static volatile sig_atomic_t writingRecord = 0;
static volatile sig_atomic_t deadlinePassed = 0;

static void timeoutExit()
{
    static const char msg[] = "pdfregions: timeout\n";
    ssize_t ignored = write(diagnosticsFd, msg, sizeof(msg) - 1);
    (void)ignored;
    _exit(exitLimit);
}

static void onTimeout(int /*sig*/)
{
    if (writingRecord && !deadlinePassed) {
        deadlinePassed = 1;
        alarm(2);
        return;
    }
    timeoutExit();
}
#endif

static bool parseLimit(const char *text, long long minValue, long long maxValue, long long *value)
{
    char *end = nullptr;
    errno = 0;
    const long long parsed = strtoll(text, &end, 10);
    if (errno != 0 || end == text || *end != '\0' || parsed < minValue || parsed > maxValue) {
        return false;
    }
    *value = parsed;
    return true;
}

//------------------------------------------------------------------------
// confinement
//------------------------------------------------------------------------

// pdfregions never writes a file and never needs the network or another
// program, but it decodes hostile images. Once the PDF is open (and
// fontconfig has loaded its configuration and caches), the process drops
// the ability to do any of that, so a decoder exploit cannot rewrite
// pdftomarkdown's output folder or reach out:
//  - PR_SET_NO_NEW_PRIVS;
//  - Landlock: every filesystem write right is handled with no rule granting
//    it (no writing, creating, removing, renaming or truncating files
//    anywhere; reads stay allowed for fonts), TCP bind/connect denied, and
//    abstract unix sockets and signals scoped to this process;
//  - seccomp: execve/execveat, socket, ptrace, process_vm_readv/writev and
//    open/openat with write or create flags fail with EPERM (openat2, whose
//    flags a filter cannot read, with ENOSYS), as a second layer that also
//    holds on kernels without Landlock.
// Already open descriptors (stdout, the saved stderr) keep working.
struct Confinement
{
    int landlockAbi = 0; // 0: not applied
    bool seccomp = false;
    bool confined() const { return landlockAbi > 0 && seccomp; }
};

#ifdef __linux__
static int applyLandlock()
{
#    if defined(SYS_landlock_create_ruleset) && defined(SYS_landlock_restrict_self)
    const long abi = syscall(SYS_landlock_create_ruleset, nullptr, 0, LANDLOCK_CREATE_RULESET_VERSION);
    if (abi < 1) {
        return 0;
    }
    struct landlock_ruleset_attr attr;
    memset(&attr, 0, sizeof(attr));
    attr.handled_access_fs = LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_REMOVE_DIR | LANDLOCK_ACCESS_FS_REMOVE_FILE | LANDLOCK_ACCESS_FS_MAKE_CHAR | LANDLOCK_ACCESS_FS_MAKE_DIR | LANDLOCK_ACCESS_FS_MAKE_REG
            | LANDLOCK_ACCESS_FS_MAKE_SOCK | LANDLOCK_ACCESS_FS_MAKE_FIFO | LANDLOCK_ACCESS_FS_MAKE_BLOCK | LANDLOCK_ACCESS_FS_MAKE_SYM;
    if (abi >= 2) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_REFER;
    }
    if (abi >= 3) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
#        ifdef LANDLOCK_ACCESS_FS_IOCTL_DEV
    if (abi >= 5) {
        attr.handled_access_fs |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
#        endif
#        ifdef LANDLOCK_ACCESS_NET_BIND_TCP
    if (abi >= 4) {
        attr.handled_access_net = LANDLOCK_ACCESS_NET_BIND_TCP | LANDLOCK_ACCESS_NET_CONNECT_TCP;
    }
#        endif
#        ifdef LANDLOCK_SCOPE_SIGNAL
    if (abi >= 6) {
        attr.scoped = LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | LANDLOCK_SCOPE_SIGNAL;
    }
#        endif
    const int fd = static_cast<int>(syscall(SYS_landlock_create_ruleset, &attr, sizeof(attr), 0));
    if (fd < 0) {
        return 0;
    }
    const long rc = syscall(SYS_landlock_restrict_self, fd, 0);
    close(fd);
    return rc == 0 ? static_cast<int>(abi) : 0;
#    else
    return 0;
#    endif
}

static bool applySeccomp()
{
#    if defined(__x86_64__)
    constexpr uint32_t arch = AUDIT_ARCH_X86_64;
#    elif defined(__aarch64__)
    constexpr uint32_t arch = AUDIT_ARCH_AARCH64;
#    else
    return false;
#    endif
#    if defined(__x86_64__) || defined(__aarch64__)
    constexpr uint32_t eperm = SECCOMP_RET_ERRNO | (EPERM & SECCOMP_RET_DATA);
    constexpr uint32_t enosys = SECCOMP_RET_ERRNO | (ENOSYS & SECCOMP_RET_DATA);
    constexpr uint32_t writeFlags = O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND;
    const uint32_t denied[] = {
        SYS_execve, SYS_execveat, SYS_socket, SYS_socketpair, SYS_connect, SYS_ptrace, SYS_process_vm_readv, SYS_process_vm_writev,
#        ifdef SYS_creat
        SYS_creat,
#        endif
    };
    std::vector<sock_filter> f;
    auto stmt = [&f](uint16_t code, uint32_t k) { f.push_back(BPF_STMT(code, k)); };
    auto jump = [&f](uint16_t code, uint32_t k, uint8_t jt, uint8_t jf) { f.push_back(BPF_JUMP(code, k, jt, jf)); };
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, arch));
    jump(BPF_JMP | BPF_JEQ | BPF_K, arch, 1, 0);
    stmt(BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS);
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr));
#        ifdef __x86_64__
    // x32 syscalls on x86_64
    jump(BPF_JMP | BPF_JGE | BPF_K, 0x40000000u, 0, 1);
    stmt(BPF_RET | BPF_K, eperm);
#        endif
    for (uint32_t nr : denied) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 1);
        stmt(BPF_RET | BPF_K, eperm);
    }
#        ifdef SYS_openat2
    jump(BPF_JMP | BPF_JEQ | BPF_K, SYS_openat2, 0, 1);
    stmt(BPF_RET | BPF_K, enosys);
#        endif
    // open(path, flags) / openat(dirfd, path, flags): deny write or create flags
    auto openCheck = [&](uint32_t nr, int flagsArg) {
        jump(BPF_JMP | BPF_JEQ | BPF_K, nr, 0, 5);
        stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + 8 * flagsArg); // low 32 bits (little endian)
        stmt(BPF_ALU | BPF_AND | BPF_K, writeFlags);
        jump(BPF_JMP | BPF_JEQ | BPF_K, 0, 1, 0);
        stmt(BPF_RET | BPF_K, eperm);
        stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    };
#        ifdef SYS_open
    openCheck(SYS_open, 1);
    stmt(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr));
#        endif
    openCheck(SYS_openat, 2);
    stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW);
    struct sock_fprog prog = { static_cast<unsigned short>(f.size()), f.data() };
    return prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog, 0, 0) == 0;
#    endif
}
#endif

static Confinement confine()
{
    Confinement c;
#ifdef __linux__
#    if WITH_FONTCONFIGURATION_FONTCONFIG
    // load fontconfig's configuration and caches now; it may want to write a
    // cache, which is not possible afterwards
    FcInit();
#    endif
    if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0) {
        return c;
    }
    c.landlockAbi = applyLandlock();
    c.seccomp = applySeccomp();
#endif
    return c;
}

//------------------------------------------------------------------------
// jobs
//------------------------------------------------------------------------

enum class Mode
{
    Raster,
    Svg,
};

struct Job
{
    std::string id;
    int page;
    double x0, y0, x1, y1;
    Mode mode;
    double dpi;
};

// ^p[0-9]{1,9}\.g?[0-9]{1,6}$
static bool validId(const std::string &s)
{
    size_t i = 0;
    if (s.size() < 4 || s[i++] != 'p') {
        return false;
    }
    size_t digits = 0;
    while (i < s.size() && s[i] >= '0' && s[i] <= '9') {
        ++i;
        ++digits;
    }
    if (digits < 1 || digits > 9 || i >= s.size() || s[i++] != '.') {
        return false;
    }
    if (i < s.size() && s[i] == 'g') {
        ++i;
    }
    digits = 0;
    while (i < s.size() && s[i] >= '0' && s[i] <= '9') {
        ++i;
        ++digits;
    }
    return digits >= 1 && digits <= 6 && i == s.size();
}

// 1-based page: [1-9][0-9]{0,8}
static bool parsePage(const std::string &s, int *page)
{
    if (s.empty() || s.size() > 9 || s[0] < '1' || s[0] > '9') {
        return false;
    }
    long v = 0;
    for (char c : s) {
        if (c < '0' || c > '9') {
            return false;
        }
        v = v * 10 + (c - '0');
    }
    *page = static_cast<int>(v);
    return true;
}

// A plain decimal: -?[0-9]{1,9}(\.[0-9]{1,9})? (no exponent, inf or nan).
static bool parseNumber(const std::string &s, double *value)
{
    size_t i = 0;
    if (i < s.size() && s[i] == '-') {
        ++i;
    }
    size_t intDigits = 0;
    while (i < s.size() && s[i] >= '0' && s[i] <= '9') {
        ++i;
        ++intDigits;
    }
    if (intDigits < 1 || intDigits > 9) {
        return false;
    }
    if (i < s.size()) {
        if (s[i++] != '.') {
            return false;
        }
        size_t fracDigits = 0;
        while (i < s.size() && s[i] >= '0' && s[i] <= '9') {
            ++i;
            ++fracDigits;
        }
        if (fracDigits < 1 || fracDigits > 9 || i != s.size()) {
            return false;
        }
    }
    char *end = nullptr;
    const double v = strtod(s.c_str(), &end);
    if (end != s.c_str() + s.size() || !std::isfinite(v)) {
        return false;
    }
    *value = v;
    return true;
}

static bool parseJob(const std::string &line, Job *job)
{
    std::vector<std::string> f;
    size_t start = 0;
    while (true) {
        const size_t tab = line.find('\t', start);
        f.push_back(line.substr(start, tab == std::string::npos ? std::string::npos : tab - start));
        if (tab == std::string::npos) {
            break;
        }
        start = tab + 1;
        if (f.size() > 8) {
            return false;
        }
    }
    if (f.size() != 8 || !validId(f[0]) || !parsePage(f[1], &job->page)) {
        return false;
    }
    job->id = f[0];
    if (!parseNumber(f[2], &job->x0) || !parseNumber(f[3], &job->y0) || !parseNumber(f[4], &job->x1) || !parseNumber(f[5], &job->y1)) {
        return false;
    }
    if (!(job->x1 > job->x0) || !(job->y1 > job->y0)) {
        return false;
    }
    if (f[6] == "raster") {
        job->mode = Mode::Raster;
    } else if (f[6] == "svg") {
        job->mode = Mode::Svg;
    } else {
        return false;
    }
    if (!parseNumber(f[7], &job->dpi)) {
        return false;
    }
    if (job->mode == Mode::Raster && !(job->dpi >= 36 && job->dpi <= 600)) {
        return false;
    }
    return true;
}

// Reads every job before rendering anything, so a malformed line (or too many
// jobs) is exit 2 with nothing on stdout.
static const char *readJobs(FILE *in, std::vector<Job> *jobs)
{
    std::string line;
    int prevPage = 0;
    int c;
    while ((c = getc(in)) != EOF) {
        if (c != '\n') {
            if (line.size() >= maxLineLength) {
                return "job line too long";
            }
            line.push_back(static_cast<char>(c));
            continue;
        }
        if (jobs->size() >= maxJobs) {
            return "too many jobs";
        }
        Job job;
        if (!parseJob(line, &job)) {
            return "malformed job line";
        }
        if (job.page < prevPage) {
            return "jobs not sorted by page";
        }
        prevPage = job.page;
        jobs->push_back(std::move(job));
        line.clear();
    }
    if (ferror(in)) {
        return "cannot read jobs";
    }
    if (!line.empty()) {
        return "malformed job line";
    }
    return nullptr;
}

//------------------------------------------------------------------------
// records
//------------------------------------------------------------------------

static bool writeAll(int fd, const char *data, size_t len)
{
    while (len > 0) {
        const ssize_t n = write(fd, data, len);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        data += n;
        len -= static_cast<size_t>(n);
    }
    return true;
}

static bool outputFailed = false;
static unsigned long long outputBytes = 0;
static unsigned long long maxOutputBytes = 0;

static void emitRecord(const Job &job, const char *status, const char *kind, int width, int height, const char *payload, size_t bytes)
{
    if (outputFailed) {
        return;
    }
    const std::string header = job.id + "\t" + status + "\t" + kind + "\t" + std::to_string(width) + "\t" + std::to_string(height) + "\t" + std::to_string(bytes) + "\n";
    if (outputBytes + header.size() + bytes > maxOutputBytes) {
        writeDiagnostic("output limit");
        _exit(exitLimit);
    }
    outputBytes += header.size() + bytes;
#ifndef _WIN32
    writingRecord = 1;
    outputFailed = !writeAll(1, header.data(), header.size()) || (bytes > 0 && !writeAll(1, payload, bytes));
    writingRecord = 0;
    if (deadlinePassed) {
        timeoutExit();
    }
#else
    outputFailed = fwrite(header.data(), 1, header.size(), stdout) != header.size() || (bytes > 0 && fwrite(payload, 1, bytes, stdout) != bytes);
#endif
}

static void emitError(const Job &job, const char *code)
{
    const std::string status = std::string("err:") + code;
    emitRecord(job, status.c_str(), "-", 0, 0, nullptr, 0);
}

//------------------------------------------------------------------------
// geometry
//------------------------------------------------------------------------

// A region cut to the page, in pdftotext word coordinates (points, y down,
// page rotation applied, media box).
struct Region
{
    double x0, y0, x1, y1;
};

static std::optional<Region> clipToPage(const Job &job, double pageW, double pageH)
{
    Region r { std::max(job.x0, 0.0), std::max(job.y0, 0.0), std::min(job.x1, pageW), std::min(job.y1, pageH) };
    if (!(r.x1 > r.x0) || !(r.y1 > r.y0)) {
        return std::nullopt;
    }
    return r;
}

// Device pixels for a region at dpi: from floor(x0*dpi/72) (a hair of tolerance
// absorbs float noise), round((x1-x0)*dpi/72) wide (at least 1), cut to the page
// bitmap; likewise y. pdftomarkdown expects that size (within 1 px).
struct PixelRect
{
    long long x, y, w, h;
    long long area() const { return w > (1LL << 31) || h > (1LL << 31) ? LLONG_MAX : w * h; }
    // Splash takes the slice as ints
    bool fitsInt() const { return x + w <= INT_MAX && y + h <= INT_MAX; }
};

static PixelRect pixelRect(const Region &r, double dpi, double pageW, double pageH)
{
    const double s = dpi / 72.0;
    constexpr double eps = 1e-6;
    // regions are at most 10^9 pt and dpi 600; a larger page cannot matter
    const long long pageWpx = static_cast<long long>(std::ceil(std::min(pageW * s, 1e12) - eps));
    const long long pageHpx = static_cast<long long>(std::ceil(std::min(pageH * s, 1e12) - eps));
    long long px0 = static_cast<long long>(std::floor(r.x0 * s + eps));
    long long py0 = static_cast<long long>(std::floor(r.y0 * s + eps));
    px0 = std::clamp(px0, 0LL, std::max(pageWpx - 1, 0LL));
    py0 = std::clamp(py0, 0LL, std::max(pageHpx - 1, 0LL));
    // the size is the region's own size at dpi, rounded (at least 1), kept on the page
    const long long wpx = std::max(1LL, std::llround((r.x1 - r.x0) * s));
    const long long hpx = std::max(1LL, std::llround((r.y1 - r.y0) * s));
    const long long px1 = std::max(std::min(px0 + wpx, pageWpx), px0 + 1);
    const long long py1 = std::max(std::min(py0 + hpx, pageHpx), py0 + 1);
    return { px0, py0, px1 - px0, py1 - py0 };
}

//------------------------------------------------------------------------
// rendering
//------------------------------------------------------------------------

// -no-images: every image call goes to OutputDev's default, which only skips
// an inline image's bytes in the content stream; image data is never decoded.
template<class Base>
class NoImages : public Base
{
public:
    using Base::Base;
    void drawImageMask(GfxState *state, Object *ref, Stream *str, int width, int height, bool invert, bool interpolate, bool inlineImg) override { OutputDev::drawImageMask(state, ref, str, width, height, invert, interpolate, inlineImg); }
    void drawImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, const int *maskColors, bool inlineImg) override
    {
        OutputDev::drawImage(state, ref, str, width, height, colorMap, interpolate, maskColors, inlineImg);
    }
    void drawMaskedImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, Stream *maskStr, int maskWidth, int maskHeight, bool maskInvert, bool maskInterpolate) override
    {
        OutputDev::drawMaskedImage(state, ref, str, width, height, colorMap, interpolate, maskStr, maskWidth, maskHeight, maskInvert, maskInterpolate);
    }
    void drawSoftMaskedImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, Stream *maskStr, int maskWidth, int maskHeight, GfxImageColorMap *maskColorMap, bool maskInterpolate) override
    {
        OutputDev::drawSoftMaskedImage(state, ref, str, width, height, colorMap, interpolate, maskStr, maskWidth, maskHeight, maskColorMap, maskInterpolate);
    }
};

class Renderer
{
public:
    Renderer(PDFDoc *docA, long long maxPixelsA, size_t maxSvgBytesA) : doc(docA), maxPixels(maxPixelsA), maxSvgBytes(maxSvgBytesA)
    {
        SplashColor white;
        white[0] = white[1] = white[2] = 255;
        if (noImages) {
            splash = std::make_unique<NoImages<SplashOutputDev>>(splashModeRGB8, 4, white, true);
        } else {
            splash = std::make_unique<SplashOutputDev>(splashModeRGB8, 4, white, true);
        }
        splash->setFontAntialias(true);
        splash->setVectorAntialias(true);
        splash->startDoc(doc);
        if (FT_Init_FreeType(&ftLib) == 0) {
            cairoFonts = std::make_unique<CairoFontEngine>(ftLib);
        }
    }

    ~Renderer()
    {
        cairoFonts.reset();
        if (ftLib) {
            FT_Done_FreeType(ftLib);
        }
    }

    // Answers every job of one page, in order. jobs[first, last) share a page.
    void renderPage(const std::vector<Job> &jobs, size_t first, size_t last);

private:
    void renderRaster(const Job &job, const PixelRect &rect);
    void renderSvg(const Job &job, const Region &r);

    PDFDoc *doc;
    long long maxPixels;
    size_t maxSvgBytes;
    std::unique_ptr<SplashOutputDev> splash;
    // Fonts are shared across SVG jobs; each job gets a fresh CairoOutputDev,
    // because a device reused across cairo surfaces keeps per-page cairo
    // state alive and leaks it (~80 KB per job, seen with LeakSanitizer).
    FT_Library ftLib = nullptr;
    std::unique_ptr<CairoFontEngine> cairoFonts;
    std::vector<unsigned char> rowBuffer;
};

// Each crop is its own slice render, exactly as pdftoppm -r -x -y -W -H draws
// it: the same box and rotation as pdftotext (media box, page /Rotate, no crop
// box). Cropping several figures out of one larger render was tried; it saves
// little (raster is cheap next to SVG) and Splash then resamples images
// against a different clip, so pixels drift from a direct render.
void Renderer::renderRaster(const Job &job, const PixelRect &rect)
{
    if (!rect.fitsInt()) {
        emitError(job, "render");
        return;
    }
    const int x = static_cast<int>(rect.x), y = static_cast<int>(rect.y), w = static_cast<int>(rect.w), h = static_cast<int>(rect.h);
    doc->displayPageSlice(splash.get(), job.page, job.dpi, job.dpi, 0, true, false, false, x, y, w, h);
    const SplashBitmap *bitmap = splash->getBitmap();
    if (!bitmap || bitmap->getMode() != splashModeRGB8 || bitmap->getWidth() != w || bitmap->getHeight() != h || !bitmap->getDataPtr() || bitmap->getRowSize() / 3 < w) {
        emitError(job, "render");
        return;
    }
    const size_t rowBytes = static_cast<size_t>(w) * 3;
    const size_t stride = static_cast<size_t>(bitmap->getRowSize());
    const SplashColorConstPtr data = bitmap->getDataPtr();
    rowBuffer.resize(rowBytes * static_cast<size_t>(h));
    for (size_t row = 0; row < static_cast<size_t>(h); ++row) {
        memcpy(rowBuffer.data() + row * rowBytes, data + row * stride, rowBytes);
    }
    emitRecord(job, "ok", "rgb8", w, h, reinterpret_cast<const char *>(rowBuffer.data()), rowBuffer.size());
    if (rowBuffer.capacity() > (64u << 20)) {
        std::vector<unsigned char>().swap(rowBuffer);
    }
}

static cairo_status_t appendToString(void *closure, const unsigned char *data, unsigned int length)
{
    static_cast<std::string *>(closure)->append(reinterpret_cast<const char *>(data), length);
    return CAIRO_STATUS_SUCCESS;
}

// A coordinate for the SVG root: at most 6 decimals, trailing zeros dropped
// (the region's own numbers, without float noise from subtraction).
static std::string formatNumber(double v)
{
    char buf[48];
    snprintf(buf, sizeof(buf), "%.6f", v);
    std::string s(buf);
    while (!s.empty() && s.back() == '0') {
        s.pop_back();
    }
    if (!s.empty() && s.back() == '.') {
        s.pop_back();
    }
    return s == "-0" ? "0" : s;
}

void Renderer::renderSvg(const Job &job, const Region &r)
{
    const double w = r.x1 - r.x0, h = r.y1 - r.y0;
    std::string out;
    cairo_surface_t *surface = cairo_svg_surface_create_for_stream(appendToString, &out, w, h);
    cairo_svg_surface_restrict_to_version(surface, CAIRO_SVG_VERSION_1_2);
    cairo_svg_surface_set_document_unit(surface, CAIRO_SVG_UNIT_PT);
    if (!cairoFonts) {
        cairo_surface_destroy(surface);
        emitError(job, "render");
        return;
    }
    std::unique_ptr<CairoOutputDev> cairo = noImages ? std::make_unique<NoImages<CairoOutputDev>>() : std::make_unique<CairoOutputDev>();
    cairo->startDoc(doc, cairoFonts.get());
    // embedded images are PNG only (pdftomarkdown's SVG allowlist), never the source JPEG etc.
    cairo->setEmbedMimeData(false);
    cairo_t *cr = cairo_create(surface);
    cairo->setCairo(cr);
    // printing: keep vectors and images at their own resolution
    cairo->setPrinting(true);
    cairo_save(cr);
    cairo_translate(cr, -r.x0, -r.y0);
    cairo_rectangle(cr, r.x0, r.y0, w, h);
    cairo_clip(cr);
    doc->displayPageSlice(cairo.get(), job.page, 72.0, 72.0, 0, true, false, false, -1, -1, -1, -1);
    cairo_restore(cr);
    cairo->setCairo(nullptr);
    cairo.reset();
    cairo_status_t status = cairo_status(cr);
    cairo_destroy(cr);
    cairo_surface_finish(surface);
    if (status == CAIRO_STATUS_SUCCESS) {
        status = cairo_surface_status(surface);
    }
    cairo_surface_destroy(surface);
    if (status != CAIRO_STATUS_SUCCESS) {
        emitError(job, "render");
        return;
    }

    // cairo draws the region at the origin (viewBox "0 0 w h"). Put it back
    // in page coordinates: viewBox = the region, content translated by its
    // origin, so the SVG's user space is the pdftotext word space.
    const size_t svgStart = out.find("<svg");
    const size_t svgEnd = svgStart == std::string::npos ? std::string::npos : out.find('>', svgStart);
    const size_t viewBox = svgStart == std::string::npos ? std::string::npos : out.find(" viewBox=\"", svgStart);
    const size_t close = out.rfind("</svg>");
    if (svgEnd == std::string::npos || viewBox == std::string::npos || viewBox > svgEnd || close == std::string::npos || close < svgEnd) {
        emitError(job, "render");
        return;
    }
    const size_t valueStart = viewBox + 10;
    const size_t valueEnd = out.find('"', valueStart);
    if (valueEnd == std::string::npos || valueEnd > svgEnd) {
        emitError(job, "render");
        return;
    }
    const std::string box = formatNumber(r.x0) + " " + formatNumber(r.y0) + " " + formatNumber(w) + " " + formatNumber(h);
    std::string svg;
    svg.reserve(out.size() + 96);
    svg.append(out, 0, valueStart);
    svg += box;
    svg.append(out, valueEnd, svgEnd + 1 - valueEnd);
    svg += "\n<g transform=\"translate(" + formatNumber(r.x0) + " " + formatNumber(r.y0) + ")\">";
    svg.append(out, svgEnd + 1, close - (svgEnd + 1));
    svg += "</g>\n";
    svg.append(out, close, std::string::npos);
    std::string().swap(out);
    if (svg.size() > maxSvgBytes) {
        emitError(job, "svgsize");
        return;
    }
    emitRecord(job, "ok", "svg", 0, 0, svg.data(), svg.size());
}

void Renderer::renderPage(const std::vector<Job> &jobs, size_t first, size_t last)
{
    const int page = jobs[first].page;
    if (page > doc->getNumPages() || !doc->getPage(page)) {
        for (size_t i = first; i < last; ++i) {
            emitError(jobs[i], "page");
        }
        return;
    }
    const double pageW = doc->getPageMediaWidth(page), pageH = doc->getPageMediaHeight(page);
    const int rotate = doc->getPageRotate(page);
    const bool swap = rotate == 90 || rotate == 270;
    const double w = swap ? pageH : pageW, h = swap ? pageW : pageH;
    if (!std::isfinite(w) || !std::isfinite(h) || !(w > 0) || !(h > 0)) {
        for (size_t i = first; i < last; ++i) {
            emitError(jobs[i], "render");
        }
        return;
    }

    for (size_t i = first; i < last; ++i) {
        const Job &job = jobs[i];
        const std::optional<Region> region = clipToPage(job, w, h);
        if (!region) {
            emitError(job, "empty");
        } else if (job.mode == Mode::Svg) {
            renderSvg(job, *region);
        } else {
            const PixelRect rect = pixelRect(*region, job.dpi, w, h);
            if (rect.area() > maxPixels) {
                emitError(job, "pixels");
            } else {
                renderRaster(job, rect);
            }
        }
    }
}

int main(int argc, char *argv[])
{
    Win32Console win32Console(&argc, &argv);

    long long maxMemory = 0, maxPixels = 0, timeout = 0, maxSvgBytes = 0, maxOutput = 0;
    const bool ok = parseArgs(argDesc, &argc, argv);
    const bool confineOk = strcmp(confineArg, "require") == 0 || strcmp(confineArg, "auto") == 0 || strcmp(confineArg, "off") == 0;
    const bool limitsOk = confineOk && parseLimit(maxMemoryArg, 0, 1LL << 30, &maxMemory) && parseLimit(maxPixelsArg, 1, 1LL << 30, &maxPixels) && parseLimit(timeoutArg, 0, 86400, &timeout) && parseLimit(maxSvgBytesArg, 1, 1LL << 40, &maxSvgBytes) && parseLimit(maxOutputBytesArg, 1, 1LL << 50, &maxOutput);
    if (!ok || !limitsOk || argc != 2 || printVersion || printHelp) {
        fprintf(stderr, "pdfregions version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdfregions", "<PDF-file>", argDesc);
            fprintf(stderr, "Jobs on stdin, records on stdout; see pdfregions(1). Exit codes: 0 ok, 1 cannot open or decrypt, 2 usage or malformed job, 3 limit (memory, timeout), 4 stdout write failure, 5 -confine require could not confine.\n");
        }
        return (printVersion || printHelp) && ok && limitsOk ? exitOk : exitUsage;
    }

    silenceStderr();
#ifndef _WIN32
    signal(SIGPIPE, SIG_IGN);
#    ifdef __linux__
    // die with the parent (pdftomarkdown) instead of lingering as an orphan;
    // if the parent is already gone, stop now
    const pid_t parent = getppid();
    prctl(PR_SET_PDEATHSIG, SIGKILL, 0, 0, 0);
    if (getppid() != parent) {
        return fail(exitLimit, "parent exited");
    }
#    endif
    if (timeout > 0) {
        struct sigaction sa;
        memset(&sa, 0, sizeof(sa));
        sa.sa_handler = onTimeout;
        sigemptyset(&sa.sa_mask);
        sigaction(SIGALRM, &sa, nullptr);
        alarm(static_cast<unsigned>(timeout));
    }
#endif
    maxOutputBytes = static_cast<unsigned long long>(maxOutput);
    limitMemory(maxMemory);
    std::set_new_handler(outOfMemory);

    std::vector<Job> jobs;
    if (const char *problem = readJobs(stdin, &jobs)) {
        return fail(exitUsage, problem);
    }

    globalParams = std::make_unique<GlobalParams>();
    setErrorCallback(quietErrorCallback);

    std::optional<GooString> ownerPW, userPW;
    if (ownerPassword[0] != '\001') {
        ownerPW = GooString(ownerPassword);
    }
    if (userPassword[0] != '\001') {
        userPW = GooString(userPassword);
    }
    std::unique_ptr<PDFDoc> doc = PDFDocFactory().createPDFDoc(GooString(argv[1]), ownerPW, userPW);
    if (!doc || !doc->isOk()) {
        return fail(exitOpenError, doc && doc->getErrorCode() == errEncrypted ? "cannot decrypt document" : "cannot open document");
    }

    if (strcmp(confineArg, "off") != 0) {
        const Confinement c = confine();
        char line[64];
        snprintf(line, sizeof(line), "confined=%d landlock=%d seccomp=%d", c.confined() ? 1 : 0, c.landlockAbi, c.seccomp ? 1 : 0);
        writeDiagnostic(line);
        if (!c.confined() && strcmp(confineArg, "require") == 0) {
            return fail(exitUnconfined, "confinement required but not available");
        }
    }
#ifdef __linux__
    if (confineCheckPath[0]) {
        // each probe reports allowed or denied; nothing else is printed
        const int fd = open(confineCheckPath, O_WRONLY | O_CREAT | O_EXCL, 0600);
        const bool created = fd >= 0;
        if (created) {
            close(fd);
        }
        const long fd2 = syscall(SYS_openat, AT_FDCWD, confineCheckPath, O_WRONLY | O_CREAT, 0600);
        if (fd2 >= 0) {
            close(static_cast<int>(fd2));
        }
        const int sock = socket(AF_INET, SOCK_STREAM, 0);
        if (sock >= 0) {
            close(sock);
        }
        // exec is probed only when confined (otherwise it would replace this process)
        bool execProbed = false;
        if (strcmp(confineArg, "off") != 0 && prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) == 1) {
            char *const argvTrue[] = { const_cast<char *>("/bin/true"), nullptr };
            execv("/bin/true", argvTrue); // returns only when denied
            execProbed = true;
        }
        char line[128];
        snprintf(line, sizeof(line), "confine-check create=%s openat=%s socket=%s exec=%s", created ? "allowed" : "denied", fd2 >= 0 ? "allowed" : "denied", sock >= 0 ? "allowed" : "denied", execProbed ? "denied" : "skipped");
        writeDiagnostic(line);
    }
#endif

    {
        Renderer renderer(doc.get(), maxPixels, static_cast<size_t>(maxSvgBytes));
        for (size_t first = 0; first < jobs.size() && !outputFailed;) {
            size_t last = first + 1;
            while (last < jobs.size() && jobs[last].page == jobs[first].page) {
                ++last;
            }
            renderer.renderPage(jobs, first, last);
            first = last;
        }
    }
    if (outputFailed) {
        return fail(exitWriteError, "cannot write output");
    }
    return exitOk;
}
