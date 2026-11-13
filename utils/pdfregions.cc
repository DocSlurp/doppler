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
#include <charconv>
#include <cstdarg>
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
#include <set>
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
#include "GfxState.h"
#include "Gfx.h"
#include "Stream.h"
#include "XRef.h"
#include "PageFactsHooks.h"
#include "ScanCopyAudit.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include "Win32Console.h"
#include "Confine.h"

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
static char maxCopyBytesArg[32] = "67108864";
static char maxCopyPixelsArg[32] = "16777216";
static bool noImages = false;
static char confineCheckPath[4096] = "";

static const ArgDesc argDesc[] = {
    { .arg = "-opw", .kind = argString, .val = ownerPassword, .size = sizeof(ownerPassword), .usage = "owner password (for encrypted files)" },
    { .arg = "-upw", .kind = argString, .val = userPassword, .size = sizeof(userPassword), .usage = "user password (for encrypted files)" },
    { .arg = "-max-memory", .kind = argString, .val = maxMemoryArg, .size = sizeof(maxMemoryArg), .usage = "address-space limit in MiB, 0 for none (default 2048)" },
    { .arg = "-max-pixels", .kind = argString, .val = maxPixelsArg, .size = sizeof(maxPixelsArg), .usage = "largest raster crop in pixels (default 16777216)" },
    { .arg = "-timeout", .kind = argString, .val = timeoutArg, .size = sizeof(timeoutArg), .usage = "wall-clock limit for the whole run in seconds, 0 for none (default 120)" },
    { .arg = "-max-svg-bytes", .kind = argString, .val = maxSvgBytesArg, .size = sizeof(maxSvgBytesArg), .usage = "largest SVG record; larger is err:svgsize (default 33554432)" },
    { .arg = "-max-output-bytes", .kind = argString, .val = maxOutputBytesArg, .size = sizeof(maxOutputBytesArg), .usage = "total stdout bytes; a record past it exits 3 (default 536870912)" },
    { .arg = "-max-copy-bytes", .kind = argString, .val = maxCopyBytesArg, .size = sizeof(maxCopyBytesArg), .usage = "largest copied JPEG stream (copy jobs); larger is err:copysize (default 67108864)" },
    { .arg = "-max-copy-pixels",
      .kind = argString,
      .val = maxCopyPixelsArg,
      .size = sizeof(maxCopyPixelsArg),
      .usage = "largest image decoded by a copy job, in pixels (8 times that for 1-bit images); larger is err:pixels (default 16777216)" },
    { .arg = "-no-images", .kind = argFlag, .val = &noImages, .size = 0, .usage = "draw no images (vector-only renders; image data is never decoded)" },
    { .arg = "-confine-check", .kind = argString, .val = confineCheckPath, .size = sizeof(confineCheckPath), .usage = "test aid: after confining, try to create this file, run a program and open a socket; report on stderr" },
    { .arg = "-confine", .kind = argString, .val = confineArg, .size = sizeof(confineArg), .usage = "no-write sandbox after opening the PDF: require, auto or off (default auto)" },
    { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
    { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    {}
};

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
// poppler's messages are not printed; their count tells a pass over a page that met a damaged
// object (a broken font, a bad stream), which the SVG jobs then render one at a time
static unsigned long errorsSeen = 0;
static void quietErrorCallback(ErrorCategory /*category*/, Goffset /*pos*/, const char * /*msg*/)
{
    ++errorsSeen;
}

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
// jobs
//------------------------------------------------------------------------

enum class Mode
{
    Raster,
    Svg,
    // an image XObject's own pixels, by reference (no rendering): see Renderer::copyImage
    Copy,
};

struct Job
{
    std::string id;
    int page;
    double x0, y0, x1, y1;
    Mode mode;
    double dpi;
    Ref ref = Ref::INVALID(); // copy jobs: the image XObject
};

// A copy job's object: <num>.<gen>, num [1-9][0-9]{0,8}, gen [0-9]{1,5} (at most 65535).
static bool parseRef(const std::string &s, Ref *ref)
{
    const size_t dot = s.find('.');
    if (dot == std::string::npos || dot == 0 || dot > 9 || s.size() - dot - 1 < 1 || s.size() - dot - 1 > 5 || s[0] == '0') {
        return false;
    }
    long num = 0, gen = 0;
    for (size_t i = 0; i < s.size(); ++i) {
        if (i == dot) {
            continue;
        }
        if (s[i] < '0' || s[i] > '9') {
            return false;
        }
        long &v = i < dot ? num : gen;
        v = v * 10 + (s[i] - '0');
    }
    if (gen > 65535) {
        return false;
    }
    ref->num = static_cast<int>(num);
    ref->gen = static_cast<int>(gen);
    return true;
}

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
    } else if (f[6] == "copy") {
        job->mode = Mode::Copy;
        job->dpi = 0;
        return parseRef(f[7], &job->ref);
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
// draw the SVG jobs of a page in one pass (PDFREGIONS_SVG_TOGETHER=0: one pass per job)
static bool svgTogether = true;
// at most this many SVG jobs share one pass: bounds the surfaces alive at once, and lets
// records flow before a long page is done (a timeout keeps the records already written)
static constexpr size_t svgChunk = 16;
// PDFREGIONS_SVG_STATS=1: one diagnostic line with the pages drawn together and the fallbacks (tests)
static bool svgStats = false;
static long svgPagesTogether = 0, svgPagesFallback = 0, svgRetries = 0;
// Memory a pass over a page for several SVG jobs may add to the process's resident size, in
// KiB: min(64 MiB, -max-memory / 16). Each device holds its own copy of everything drawn (and
// cairo decodes and encodes the images of each when it finishes), so past this the pass stops
// and the jobs render one at a time, as one job would alone; the finished SVGs a pass keeps for
// later records count against it too.
static long long svgSlackKiB = 64 * 1024;
// /proc/self/statm, opened before confinement; -1 when unavailable (then no pass is shared)
static int statmFd = -1;

// The process's resident size in KiB now, or -1 when unknown. (getrusage's peak is no use: it
// keeps the high-water mark of the process before exec, such as a large parent's.)
static long long residentKiB()
{
#ifndef _WIN32
    if (statmFd >= 0) {
        char buf[128];
        const ssize_t n = pread(statmFd, buf, sizeof(buf) - 1, 0);
        if (n > 0) {
            buf[n] = '\0';
            unsigned long long size = 0, resident = 0;
            if (sscanf(buf, "%llu %llu", &size, &resident) == 2) {
                static const long long pageKiB = sysconf(_SC_PAGESIZE) / 1024;
                return static_cast<long long>(resident) * pageKiB;
            }
        }
    }
#endif
    return -1;
}
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
// fixed-point printing
//------------------------------------------------------------------------

// cairo's SVG surface prints every coordinate with snprintf "%f" (and numbers
// under 0.1 with "%.18f", then "%.*f"); glibc formats those through its
// multi-precision code, which was about a tenth of the image pass on figure-
// heavy documents. This process (C locale, default rounding) answers a format
// that is exactly "%f", "%.<N>f" or "%.*f" with a finite double through
// std::to_chars(fixed, precision): correctly rounded like glibc, so the same
// characters; anything else goes to glibc. Defining the functions here makes
// libcairo and libpoppler call them (an executable's definitions come first).
namespace {

// The precision of a whole format "%f" (6), "%.<N>f" (N), "%.*f" (*star), else -1.
int fixedFormat(const char *fmt, bool *star)
{
    *star = false;
    if (fmt[0] != '%') {
        return -1;
    }
    if (fmt[1] == 'f' && fmt[2] == '\0') {
        return 6;
    }
    if (fmt[1] != '.') {
        return -1;
    }
    if (fmt[2] == '*' && fmt[3] == 'f' && fmt[4] == '\0') {
        *star = true;
        return 0;
    }
    int prec = 0, i = 2;
    for (; fmt[i] >= '0' && fmt[i] <= '9' && i < 5; ++i) {
        prec = prec * 10 + (fmt[i] - '0');
    }
    return i > 2 && fmt[i] == 'f' && fmt[i + 1] == '\0' ? prec : -1;
}

// snprintf's answer for a fixed format, or -1 to leave it to glibc.
int fixedPrint(char *s, size_t maxlen, int prec, double v)
{
    if (prec < 0 || prec > 60 || !std::isfinite(v)) {
        return -1;
    }
    char buf[400];
    const std::to_chars_result r = std::to_chars(buf, buf + sizeof(buf), v, std::chars_format::fixed, prec);
    if (r.ec != std::errc()) {
        return -1;
    }
    const size_t n = static_cast<size_t>(r.ptr - buf);
    if (maxlen > 0) {
        const size_t k = std::min(n, maxlen - 1);
        memcpy(s, buf, k);
        s[k] = '\0';
    }
    return static_cast<int>(n);
}

bool fixedPrintingOff()
{
    static const bool off = [] {
        const char *e = getenv("PDFREGIONS_GLIBC_PRINTF");
        return e && strcmp(e, "1") == 0;
    }();
    return off;
}

} // namespace

extern "C" int __vsnprintf_chk(char *s, size_t maxlen, int flag, size_t slen, const char *fmt, va_list ap);
// default visibility: the build hides symbols (-fvisibility=hidden), and these must be exported
extern "C" __attribute__((visibility("default"))) int __snprintf_chk(char *s, size_t maxlen, int flag, size_t slen, const char *fmt, ...);
extern "C" __attribute__((visibility("default"))) int snprintf(char *__restrict s, size_t maxlen, const char *__restrict fmt, ...) noexcept;

extern "C" int __snprintf_chk(char *s, size_t maxlen, int flag, size_t slen, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    bool star;
    int prec = fixedPrintingOff() || slen < maxlen ? -1 : fixedFormat(fmt, &star);
    int n = -1;
    if (prec >= 0) {
        va_list probe;
        va_copy(probe, ap);
        if (star) {
            prec = va_arg(probe, int);
        }
        n = fixedPrint(s, maxlen, prec, va_arg(probe, double));
        va_end(probe);
    }
    if (n < 0) {
        n = __vsnprintf_chk(s, maxlen, flag, slen, fmt, ap);
    }
    va_end(ap);
    return n;
}

extern "C" int snprintf(char *__restrict s, size_t maxlen, const char *__restrict fmt, ...) noexcept
{
    va_list ap;
    va_start(ap, fmt);
    bool star;
    int prec = fixedPrintingOff() ? -1 : fixedFormat(fmt, &star);
    int n = -1;
    if (prec >= 0) {
        va_list probe;
        va_copy(probe, ap);
        if (star) {
            prec = va_arg(probe, int);
        }
        n = fixedPrint(s, maxlen, prec, va_arg(probe, double));
        va_end(probe);
    }
    if (n < 0) {
        n = vsnprintf(s, maxlen, fmt, ap);
    }
    va_end(ap);
    return n;
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

// Draws one page into several output devices at once: the page's content is
// parsed and interpreted once and every call goes to each device in turn.
// Used for the SVG jobs of one page, which all draw the whole page at 72 dpi
// with the same Gfx state and differ only in their own cairo clip, so each
// device receives exactly the calls it would get from a render of its own.
// Anything that could make the devices' inputs differ from separate renders
// marks the tee unsafe; the caller then discards every device's output and
// renders the jobs one at a time as before:
//  - an inline image: its stream replays only what the first device read;
//  - a shaded fill entered with a path (the device rewrites the state path);
//  - devices answering a question differently.
class TeeOutputDev : public OutputDev
{
public:
    TeeOutputDev(std::vector<OutputDev *> devsA, long long limitKiBA) : devs(std::move(devsA)), limitKiB(limitKiBA) { }

    bool isUnsafe() const { return unsafe; }
    bool overMemory() const { return memory; }
    // Gfx calls this about every 10 operators; memory is read every 8th call (paths and text
    // grow slowly; an image is checked before each device draws it)
    static bool abortCheck(void *data)
    {
        auto *tee = static_cast<TeeOutputDev *>(data);
        if ((++tee->checks & 7) == 0) {
            tee->checkMemory();
        }
        return tee->unsafe;
    }

    // questions: every device is the same kind with the same settings
    bool upsideDown() override { return agree([](OutputDev *d) { return d->upsideDown(); }); }
    bool useDrawChar() override { return agree([](OutputDev *d) { return d->useDrawChar(); }); }
    bool useTilingPatternFill() override { return agree([](OutputDev *d) { return d->useTilingPatternFill(); }); }
    bool useShadedFills(int type) override { return agree([type](OutputDev *d) { return d->useShadedFills(type); }); }
    bool useFillColorStop() override { return agree([](OutputDev *d) { return d->useFillColorStop(); }); }
    bool useDrawForm() override { return agree([](OutputDev *d) { return d->useDrawForm(); }); }
    bool interpretType3Chars() override { return agree([](OutputDev *d) { return d->interpretType3Chars(); }); }
    bool needNonText() override { return agree([](OutputDev *d) { return d->needNonText(); }); }
    bool needCharCount() override { return agree([](OutputDev *d) { return d->needCharCount(); }); }
    bool needClipToCropBox() override { return agree([](OutputDev *d) { return d->needClipToCropBox(); }); }
    bool supportJPXtransparency() override { return agree([](OutputDev *d) { return d->supportJPXtransparency(); }); }
    bool needImageMaskGeometry() override { return agree([](OutputDev *d) { return d->needImageMaskGeometry(); }); }
    bool needImageGeometry() override { return agree([](OutputDev *d) { return d->needImageGeometry(); }); }
    bool getVectorAntialias() override { return agree([](OutputDev *d) { return d->getVectorAntialias(); }); }
    bool checkTransparencyGroup(GfxState *state, bool knockout) override { return agree([&](OutputDev *d) { return d->checkTransparencyGroup(state, knockout); }); }
    bool axialShadedSupportExtend(GfxState *state, GfxAxialShading *shading) override { return agree([&](OutputDev *d) { return d->axialShadedSupportExtend(state, shading); }); }
    bool radialShadedSupportExtend(GfxState *state, GfxRadialShading *shading) override { return agree([&](OutputDev *d) { return d->radialShadedSupportExtend(state, shading); }); }

    void setDefaultCTM(const std::array<double, 6> &ctm) override
    {
        OutputDev::setDefaultCTM(ctm);
        each([&](OutputDev *d) { d->setDefaultCTM(ctm); });
    }
    void setVectorAntialias(bool vaa) override
    {
        each([&](OutputDev *d) { d->setVectorAntialias(vaa); });
    }

#define TEE_STATE(name)                                                                                                                                                                                                                        \
    void name(GfxState *state) override                                                                                                                                                                                                       \
    {                                                                                                                                                                                                                                          \
        each([&](OutputDev *d) { d->name(state); });                                                                                                                                                                                          \
    }
    TEE_STATE(saveState)
    TEE_STATE(restoreState)
    TEE_STATE(updateAll)
    TEE_STATE(updateLineDash)
    TEE_STATE(updateFlatness)
    TEE_STATE(updateLineJoin)
    TEE_STATE(updateLineCap)
    TEE_STATE(updateMiterLimit)
    TEE_STATE(updateLineWidth)
    TEE_STATE(updateStrokeAdjust)
    TEE_STATE(updateAlphaIsShape)
    TEE_STATE(updateTextKnockout)
    TEE_STATE(updateFillColorSpace)
    TEE_STATE(updateStrokeColorSpace)
    TEE_STATE(updateFillColor)
    TEE_STATE(updateStrokeColor)
    TEE_STATE(updateBlendMode)
    TEE_STATE(updateFillOpacity)
    TEE_STATE(updateStrokeOpacity)
    TEE_STATE(updatePatternOpacity)
    TEE_STATE(clearPatternOpacity)
    TEE_STATE(updateFillOverprint)
    TEE_STATE(updateStrokeOverprint)
    TEE_STATE(updateOverprintMode)
    TEE_STATE(updateTransfer)
    TEE_STATE(updateFont)
    TEE_STATE(updateTextMat)
    TEE_STATE(updateCharSpace)
    TEE_STATE(updateRender)
    TEE_STATE(updateRise)
    TEE_STATE(updateWordSpace)
    TEE_STATE(updateHorizScaling)
    TEE_STATE(updateTextPos)
    TEE_STATE(saveTextPos)
    TEE_STATE(restoreTextPos)
    TEE_STATE(stroke)
    TEE_STATE(fill)
    TEE_STATE(eoFill)
    TEE_STATE(clip)
    TEE_STATE(eoClip)
    TEE_STATE(clipToStrokePath)
    TEE_STATE(beginStringOp)
    TEE_STATE(endStringOp)
    TEE_STATE(endString)
    TEE_STATE(endType3Char)
    TEE_STATE(beginTextObject)
    TEE_STATE(endTextObject)
    TEE_STATE(endActualText)
    TEE_STATE(imageMaskGeometry)
    TEE_STATE(endMarkedContent)
    TEE_STATE(endTransparencyGroup)
    TEE_STATE(clearSoftMask)
#undef TEE_STATE

    void startPage(int pageNum, GfxState *state, XRef *xref) override
    {
        each([&](OutputDev *d) { d->startPage(pageNum, state, xref); });
    }
    void endPage() override
    {
        each([](OutputDev *d) { d->endPage(); });
    }
    void dump() override
    {
        each([](OutputDev *d) { d->dump(); });
    }
    void updateCTM(GfxState *state, double m11, double m12, double m21, double m22, double m31, double m32) override
    {
        each([&](OutputDev *d) { d->updateCTM(state, m11, m12, m21, m22, m31, m32); });
    }
    void updateFillColorStop(GfxState *state, double offset) override
    {
        each([&](OutputDev *d) { d->updateFillColorStop(state, offset); });
    }
    void updateTextShift(GfxState *state, double shift) override
    {
        each([&](OutputDev *d) { d->updateTextShift(state, shift); });
    }
    bool tilingPatternFill(GfxState *state, Gfx *gfx, Catalog *cat, GfxTilingPattern *tPat, const std::array<double, 6> &mat, int x0, int y0, int x1, int y1, double xStep, double yStep) override
    {
        return agree([&](OutputDev *d) { return d->tilingPatternFill(state, gfx, cat, tPat, mat, x0, y0, x1, y1, xStep, yStep); });
    }
    // these three draw the clip box through the state's path and clear it
    bool functionShadedFill(GfxState *state, GfxFunctionShading *shading) override
    {
        pathless(state);
        return agree([&](OutputDev *d) { return d->functionShadedFill(state, shading); });
    }
    bool gouraudTriangleShadedFill(GfxState *state, GfxGouraudTriangleShading *shading) override
    {
        pathless(state);
        return agree([&](OutputDev *d) { return d->gouraudTriangleShadedFill(state, shading); });
    }
    bool patchMeshShadedFill(GfxState *state, GfxPatchMeshShading *shading) override
    {
        pathless(state);
        return agree([&](OutputDev *d) { return d->patchMeshShadedFill(state, shading); });
    }
    bool axialShadedFill(GfxState *state, GfxAxialShading *shading, double tMin, double tMax) override
    {
        return agree([&](OutputDev *d) { return d->axialShadedFill(state, shading, tMin, tMax); });
    }
    bool radialShadedFill(GfxState *state, GfxRadialShading *shading, double sMin, double sMax) override
    {
        return agree([&](OutputDev *d) { return d->radialShadedFill(state, shading, sMin, sMax); });
    }
    void beginString(GfxState *state, const std::string &str) override
    {
        each([&](OutputDev *d) { d->beginString(state, str); });
    }
    void drawChar(GfxState *state, double x, double y, double dx, double dy, double originX, double originY, CharCode code, int nBytes, const Unicode *u, int uLen) override
    {
        each([&](OutputDev *d) { d->drawChar(state, x, y, dx, dy, originX, originY, code, nBytes, u, uLen); });
    }
    void drawString(GfxState *state, const std::string &str) override
    {
        each([&](OutputDev *d) { d->drawString(state, str); });
    }
    bool beginType3Char(GfxState *state, double x, double y, double dx, double dy, CharCode code, const Unicode *u, int uLen) override
    {
        return agree([&](OutputDev *d) { return d->beginType3Char(state, x, y, dx, dy, code, u, uLen); });
    }
    void incCharCount(int nChars) override
    {
        each([&](OutputDev *d) { d->incCharCount(nChars); });
    }
    void beginActualText(GfxState *state, const std::string &text) override
    {
        each([&](OutputDev *d) { d->beginActualText(state, text); });
    }
    // images: each device rewinds the stream before reading it (an XObject
    // stream rereads from the file); an inline image cannot be reread
    void drawImageMask(GfxState *state, Object *ref, Stream *str, int width, int height, bool invert, bool interpolate, bool inlineImg) override
    {
        image(inlineImg, [&](OutputDev *d) { d->drawImageMask(state, ref, str, width, height, invert, interpolate, inlineImg); });
    }
    bool setSoftMaskFromImageMask(GfxState *state, Object *ref, Stream *str, int width, int height, bool invert, bool inlineImg, std::array<double, 6> &baseMatrix) override
    {
        if (inlineImg) {
            unsafe = true;
            return devs[0]->setSoftMaskFromImageMask(state, ref, str, width, height, invert, inlineImg, baseMatrix);
        }
        const std::array<double, 6> base = baseMatrix;
        bool first = true, answer = false;
        for (OutputDev *d : devs) {
            std::array<double, 6> m = base;
            const bool r = d->setSoftMaskFromImageMask(state, ref, str, width, height, invert, inlineImg, m);
            if (first) {
                answer = r;
                baseMatrix = m;
                first = false;
            } else if (r != answer || m != baseMatrix) {
                unsafe = true;
            }
        }
        return answer;
    }
    void unsetSoftMaskFromImageMask(GfxState *state, std::array<double, 6> &baseMatrix) override
    {
        const std::array<double, 6> base = baseMatrix;
        bool first = true;
        for (OutputDev *d : devs) {
            std::array<double, 6> m = base;
            d->unsetSoftMaskFromImageMask(state, m);
            if (first) {
                baseMatrix = m;
                first = false;
            } else if (m != baseMatrix) {
                unsafe = true;
            }
        }
    }
    void drawImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, const int *maskColors, bool inlineImg) override
    {
        image(inlineImg, [&](OutputDev *d) { d->drawImage(state, ref, str, width, height, colorMap, interpolate, maskColors, inlineImg); });
    }
    void drawMaskedImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, Stream *maskStr, int maskWidth, int maskHeight, bool maskInvert, bool maskInterpolate) override
    {
        image(false, [&](OutputDev *d) { d->drawMaskedImage(state, ref, str, width, height, colorMap, interpolate, maskStr, maskWidth, maskHeight, maskInvert, maskInterpolate); });
    }
    void drawSoftMaskedImage(GfxState *state, Object *ref, Stream *str, int width, int height, GfxImageColorMap *colorMap, bool interpolate, Stream *maskStr, int maskWidth, int maskHeight, GfxImageColorMap *maskColorMap,
                             bool maskInterpolate) override
    {
        image(false, [&](OutputDev *d) { d->drawSoftMaskedImage(state, ref, str, width, height, colorMap, interpolate, maskStr, maskWidth, maskHeight, maskColorMap, maskInterpolate); });
    }
    void imageGeometry(GfxState *state, int width, int height, bool stencil, Ref ref, const ImageDictInfo &info) override
    {
        each([&](OutputDev *d) { d->imageGeometry(state, width, height, stencil, ref, info); });
    }
    void beginMarkedContent(const std::string &name, Dict *properties) override
    {
        each([&](OutputDev *d) { d->beginMarkedContent(name, properties); });
    }
    void markPoint(const std::string &name) override
    {
        each([&](OutputDev *d) { d->markPoint(name); });
    }
    void markPoint(const std::string &name, Dict *properties) override
    {
        each([&](OutputDev *d) { d->markPoint(name, properties); });
    }
    void opiBegin(GfxState *state, const Dict &opiDict) override
    {
        each([&](OutputDev *d) { d->opiBegin(state, opiDict); });
    }
    void opiEnd(GfxState *state, const Dict &opiDict) override
    {
        each([&](OutputDev *d) { d->opiEnd(state, opiDict); });
    }
    void type3D0(GfxState *state, double wx, double wy) override
    {
        each([&](OutputDev *d) { d->type3D0(state, wx, wy); });
    }
    void type3D1(GfxState *state, double wx, double wy, double llx, double lly, double urx, double ury) override
    {
        each([&](OutputDev *d) { d->type3D1(state, wx, wy, llx, lly, urx, ury); });
    }
    void beginForm(Object *obj, Ref id) override
    {
        each([&](OutputDev *d) { d->beginForm(obj, id); });
    }
    void drawForm(Ref id) override
    {
        each([&](OutputDev *d) { d->drawForm(id); });
    }
    void endForm(Object *obj, Ref id) override
    {
        each([&](OutputDev *d) { d->endForm(obj, id); });
    }
    void psXObject(Stream *psStream, Stream *level1Stream) override
    {
        // never reached with cairo devices; a stream read twice is not the same
        unsafe = true;
        devs[0]->psXObject(psStream, level1Stream);
    }
    void beginTransparencyGroup(GfxState *state, const std::array<double, 4> &bbox, GfxColorSpace *blendingColorSpace, bool isolated, bool knockout, bool forSoftMask) override
    {
        each([&](OutputDev *d) { d->beginTransparencyGroup(state, bbox, blendingColorSpace, isolated, knockout, forSoftMask); });
    }
    void paintTransparencyGroup(GfxState *state, const std::array<double, 4> &bbox) override
    {
        each([&](OutputDev *d) { d->paintTransparencyGroup(state, bbox); });
    }
    void setSoftMask(GfxState *state, const std::array<double, 4> &bbox, bool alpha, Function *transferFunc, const GfxColor &backdropColor) override
    {
        each([&](OutputDev *d) { d->setSoftMask(state, bbox, alpha, transferFunc, backdropColor); });
    }
    void processLink(AnnotLink *link) override
    {
        each([&](OutputDev *d) { d->processLink(link); });
    }

private:
    template<class F>
    void each(F &&f)
    {
        for (OutputDev *d : devs) {
            f(d);
        }
    }
    // every device is asked; the first one's answer is used, and a device
    // that answers otherwise makes the result unusable
    template<class F>
    bool agree(F &&f)
    {
        const bool answer = f(devs[0]);
        for (size_t i = 1; i < devs.size(); ++i) {
            if (f(devs[i]) != answer) {
                unsafe = true;
            }
        }
        return answer;
    }
    // an image is copied into every device: the memory is checked before each copy, and once
    // over the pass is discarded, so the other devices skip it (drawing an image changes no
    // device state)
    template<class F>
    void image(bool inlineImg, F &&f)
    {
        if (inlineImg) {
            unsafe = true;
            f(devs[0]);
            return;
        }
        for (OutputDev *d : devs) {
            if (checkMemory()) {
                return;
            }
            f(d);
        }
    }
    // true (and the pass unsafe) once the resident size passed the pass's limit
    bool checkMemory()
    {
        // an unreadable resident size counts as over the limit: the shared pass is dropped
        if (!memory) {
            const long long rss = residentKiB();
            if (rss < 0 || rss > limitKiB) {
                memory = unsafe = true;
            }
        }
        return memory;
    }
    void pathless(GfxState *state)
    {
        if (state->isCurPt() || state->isPath()) {
            unsafe = true;
        }
    }

    std::vector<OutputDev *> devs;
    long long limitKiB;
    unsigned checks = 0;
    bool unsafe = false;
    bool memory = false;
};

// The page-level check before a copy job (ScanCopyAudit.h): runs the page's content, its forms
// and its annotations as the raster renders do (media box, page /Rotate, not printing), drawing
// nothing and decoding no image, and records whether anything but one plain image XObject paints.
// pdftotext -images runs the same allow-list on its own pass (copyable="1"); this is the
// renderer's own check, so a job line cannot ask for a copy the page does not allow.
class ScanAuditOutputDev : public OutputDev
{
public:
    bool upsideDown() override { return true; }
    bool useDrawChar() override { return false; }
    bool interpretType3Chars() override { return false; }
    bool needNonText() override { return false; }
    bool needCharCount() override { return true; }
    bool checkPageSlice(Page *pageA, double /*hDPI*/, double /*vDPI*/, int /*rotate*/, bool /*useMediaBox*/, bool /*crop*/, int /*sliceX*/, int /*sliceY*/, int /*sliceW*/, int /*sliceH*/, bool /*printing*/,
                        bool (* /*abortCheckCbk*/)(void *data), void * /*abortCheckCbkData*/, bool (* /*annotDisplayDecideCbk*/)(Annot *annot, void *user_data), void * /*annotDisplayDecideCbkData*/) override
    {
        page = pageA;
        return true;
    }
    void startPage(int /*pageNum*/, GfxState *state, XRef * /*xref*/) override { audit.startPage(page, state); }
    void clip(GfxState *state) override { audit.clip(state); }
    void eoClip(GfxState *state) override { audit.clip(state); }
    void clipToStrokePath(GfxState * /*state*/) override { audit.deny(ScanCopyAudit::Reason::Clip); }
    void beginTransparencyGroup(GfxState * /*state*/, const std::array<double, 4> & /*bbox*/, GfxColorSpace * /*blendingColorSpace*/, bool /*isolated*/, bool /*knockout*/, bool /*forSoftMask*/) override
    {
        audit.deny(ScanCopyAudit::Reason::Group);
    }
    void setSoftMask(GfxState * /*state*/, const std::array<double, 4> & /*bbox*/, bool /*alpha*/, Function * /*transferFunc*/, const GfxColor & /*backdropColor*/) override { audit.deny(ScanCopyAudit::Reason::SoftMask); }
    void skippedSoftMask(GfxState * /*state*/) override { audit.deny(ScanCopyAudit::Reason::SoftMask); }
    ScanCopyAudit *getScanCopyAudit() override { return &audit; }

    ScanCopyAudit audit;

private:
    Page *page = nullptr;
};

class Renderer
{
public:
    Renderer(PDFDoc *docA, long long maxPixelsA, size_t maxSvgBytesA, size_t maxCopyBytesA, long long maxCopyPixelsA) : doc(docA), maxPixels(maxPixelsA), maxSvgBytes(maxSvgBytesA), maxCopyBytes(maxCopyBytesA), maxCopyPixels(maxCopyPixelsA)
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
    struct SvgCanvas;
    void renderRaster(const Job &job, const PixelRect &rect);
    void copyImage(const Job &job);
    void renderSvg(const Job &job, const Region &r);
    bool beginSvg(SvgCanvas &c);
    const char *finishSvg(SvgCanvas &c, std::string *svg);
    void emitSvg(const Job &job, const char *error, const std::string &svg);
    bool renderSvgTogether(const std::vector<Job> &jobs, const std::vector<std::pair<size_t, Region>> &svgJobs, std::vector<std::pair<const char *, std::string>> *results);

    PDFDoc *doc;
    long long maxPixels;
    size_t maxSvgBytes;
    size_t maxCopyBytes;
    long long maxCopyPixels;
    std::unique_ptr<SplashOutputDev> splash;
    // Fonts are shared across SVG jobs; each job gets a fresh CairoOutputDev,
    // because a device reused across cairo surfaces keeps per-page cairo
    // state alive and leaks it (~80 KB per job, seen with LeakSanitizer).
    FT_Library ftLib = nullptr;
    std::unique_ptr<CairoFontEngine> cairoFonts;
    std::vector<unsigned char> rowBuffer;
    // the copy allow-list of the last page audited (copy jobs come sorted by page)
    int auditedPage = 0;
    ScanCopyAudit pageAudit;
    // colour spaces by reference, classified once for every page audit and copy job
    ImageInfoCache imageInfoCache;
    bool pageCopyable(int pageNum, Ref ref);
};

bool Renderer::pageCopyable(int pageNum, Ref ref)
{
    if (auditedPage != pageNum) {
        ScanAuditOutputDev dev;
        dev.audit.setColorCache(&imageInfoCache);
        doc->displayPage(&dev, pageNum, 72.0, 72.0, 0, true, false, false);
        pageAudit = dev.audit;
        auditedPage = pageNum;
    }
    return pageAudit.copyable(ref);
}

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

//------------------------------------------------------------------------
// copy jobs: an image XObject's own pixels
//------------------------------------------------------------------------

// Forms nested deeper than this, entries past this many, are not searched for a copy job's image.
static constexpr int copyMaxFormDepth = 4;
static constexpr int copyMaxEntries = 10000;

// Whether image XObject <target> is named in <resources>' /XObject, directly or inside the
// resources of a form XObject named there (forms nested at most copyMaxFormDepth deep). Only
// dictionaries are read. The search goes level by level, so each form is visited once, at the
// shallowest depth it is named at; at most copyMaxEntries entries in all.
static bool xobjectReachable(XRef *xref, Dict *resources, Ref target, std::set<Ref> *forms, int *budget)
{
    // names <target> directly; collects the forms it names that were not seen yet
    auto scan = [&](Dict *res, bool nest, std::vector<Ref> *next) -> bool {
        if (!res) {
            return false;
        }
        const Object xobjects = res->lookup("XObject");
        if (!xobjects.isDict()) {
            return false;
        }
        Dict *dict = xobjects.getDict();
        for (int i = 0; i < dict->getLength(); ++i) {
            if (--*budget < 0) {
                return false;
            }
            const Object &value = dict->getValNF(i);
            if (!value.isRef()) {
                continue;
            }
            if (value.getRef() == target) {
                return true;
            }
            if (nest && forms->insert(value.getRef()).second) {
                next->push_back(value.getRef());
            }
        }
        return false;
    };
    std::vector<Ref> level;
    if (scan(resources, copyMaxFormDepth > 0, &level)) {
        return true;
    }
    for (int depth = 1; depth <= copyMaxFormDepth && !level.empty() && *budget >= 0; ++depth) {
        std::vector<Ref> next;
        for (const Ref r : level) {
            const Object form = xref->fetch(r);
            if (!form.isStream()) {
                continue;
            }
            Dict *formDict = form.getStream()->getDict();
            if (!formDict->lookup("Subtype").isName("Form")) {
                continue;
            }
            const Object formResources = formDict->lookup("Resources");
            if (formResources.isDict() && scan(formResources.getDict(), depth < copyMaxFormDepth, &next)) {
                return true;
            }
        }
        level = std::move(next);
    }
    return false;
}

// Checks a JPEG for a copy and rewrites it without metadata: SOI, a baseline, extended or
// progressive Huffman frame (SOF0..2) of 8-bit samples with <comps> components and the image's
// size, a scan, and EOI at the end (at most 64 bytes of padding after it are cut). The output
// keeps the tables (DQT, DHT, DRI), the frame, the scans and their entropy-coded data, and one
// canonical segment that decides how the samples decode as poppler decodes them: an Adobe APP14
// (its colour transform) when the stream had one, else a JFIF APP0 (version 1.01, 1:1, no
// thumbnail) when it had one libjpeg recognises, else neither (the component ids). Every other
// APPn (Exif with its orientation and thumbnail, ICC, XMP, Photoshop) and every COM is dropped,
// before the frame and between scans, so no document-supplied string or second image reaches
// the file and viewers show the pixels as the PDF draws them.
static bool cleanJpeg(const std::vector<unsigned char> &d, int width, int height, int comps, std::vector<unsigned char> *out)
{
    const size_t n = d.size();
    if (n < 4 || d[0] != 0xFF || d[1] != 0xD8) {
        return false;
    }
    // EOI ends the stream (padding of at most 64 bytes after it is cut)
    size_t end = n;
    const size_t floor = n > 66 ? n - 66 : 2;
    while (end >= floor + 2 && !(d[end - 2] == 0xFF && d[end - 1] == 0xD9)) {
        --end;
    }
    if (end < floor + 2) {
        return false;
    }
    out->clear();
    out->reserve(end);
    out->insert(out->end(), { 0xFF, 0xD8 });
    bool jfif = false, adobe = false;
    unsigned char transform = 0;
    size_t i = 2;
    bool frame = false, scan = false;
    // what a decoder checks of the tables, frame and scans (libjpeg refuses a file that breaks
    // any of these: "Bogus Huffman table", "Invalid SOS parameters", ...): the copy must be a
    // JPEG that decodes, not just one framed as segments
    bool progressive = false;
    unsigned quantDefined = 0; // bit per DQT table id
    struct Component
    {
        int id, h, v, tq;
    };
    std::vector<Component> components;
    size_t headerEnd = 0; // where the canonical APP segments go: right after SOI
    headerEnd = out->size();
    while (i < end - 2) {
        if (d[i] != 0xFF) {
            if (!scan) {
                return false; // no data before the first scan
            }
            out->push_back(d[i++]);
            continue;
        }
        const unsigned char marker = d[i + 1];
        if (marker == 0xFF) {
            ++i; // fill byte
            continue;
        }
        if (scan && (marker == 0x00 || (marker >= 0xD0 && marker <= 0xD7))) {
            // a stuffed 0xFF, or a restart marker, inside entropy-coded data
            out->insert(out->end(), { 0xFF, marker });
            i += 2;
            continue;
        }
        if (marker == 0xD8 || marker == 0xD9 || (marker >= 0xD0 && marker <= 0xD7) || marker == 0x01 || marker == 0x00) {
            return false; // no image inside, nothing before the scan, EOI only last
        }
        if (i + 4 > end) {
            return false;
        }
        const size_t len = (static_cast<size_t>(d[i + 2]) << 8) | d[i + 3];
        if (len < 2 || i + 2 + len > end) {
            return false;
        }
        const unsigned char *seg = d.data() + i;
        if (marker == 0xE0) {
            // libjpeg takes an APP0 for JFIF only with at least 14 bytes of data (APP0_DATA_LEN)
            jfif = jfif || (len >= 2 + 14 && !memcmp(seg + 4, "JFIF\0", 5));
        } else if (marker == 0xEE) {
            // and an APP14 for Adobe with at least 12 (APP14_DATA_LEN); the last one wins
            if (len >= 2 + 12 && !memcmp(seg + 4, "Adobe", 5)) {
                adobe = true;
                transform = seg[15];
            }
        } else if ((marker >= 0xE1 && marker <= 0xEF) || marker == 0xFE) {
            // other metadata: dropped
        } else if (marker == 0xC0 || marker == 0xC1 || marker == 0xC2) {
            // a frame: only Huffman baseline, extended and progressive, once, before any scan
            if (frame || scan || len < 8) {
                return false;
            }
            const int precision = seg[4];
            const int h = (seg[5] << 8) | seg[6];
            const int w = (seg[7] << 8) | seg[8];
            const int nc = seg[9];
            if (precision != 8 || w != width || h != height || nc != comps || len != 8 + 3 * static_cast<size_t>(nc)) {
                return false;
            }
            for (int c = 0; c < nc; ++c) {
                const unsigned char *cs = seg + 10 + 3 * c;
                const Component comp { cs[0], cs[1] >> 4, cs[1] & 15, cs[2] };
                if (comp.h < 1 || comp.h > 4 || comp.v < 1 || comp.v > 4 || comp.tq > 3) {
                    return false;
                }
                for (const Component &other : components) {
                    if (other.id == comp.id) {
                        return false;
                    }
                }
                components.push_back(comp);
            }
            progressive = marker == 0xC2;
            frame = true;
            out->insert(out->end(), seg, seg + 2 + len);
        } else if (marker == 0xDA) {
            if (!frame || len < 6) {
                return false;
            }
            const int ns = seg[4];
            if (ns < 1 || ns > 4 || len != 6 + 2 * static_cast<size_t>(ns)) {
                return false;
            }
            int blocks = 0;
            for (int c = 0; c < ns; ++c) {
                const int id = seg[5 + 2 * c], td = seg[6 + 2 * c] >> 4, ta = seg[6 + 2 * c] & 15;
                const Component *comp = nullptr;
                for (const Component &k : components) {
                    comp = k.id == id ? &k : comp;
                }
                for (int e = 0; e < c; ++e) {
                    if (seg[5 + 2 * e] == id) {
                        return false;
                    }
                }
                if (!comp || td > 3 || ta > 3 || !(quantDefined >> comp->tq & 1)) {
                    return false;
                }
                blocks += comp->h * comp->v;
            }
            if (ns > 1 && blocks > 10) {
                return false; // sampling factors too large for an interleaved scan
            }
            const unsigned char *tail = seg + 5 + 2 * ns;
            const int ss = tail[0], se = tail[1], ah = tail[2] >> 4, al = tail[2] & 15;
            if (progressive) {
                if (se > 63 || ss > se || (ss == 0 && se != 0) || (ss > 0 && ns != 1) || ah > 13 || al > 13 || (ah != 0 && ah != al + 1)) {
                    return false;
                }
            } else if (ss != 0 || se != 63 || ah != 0 || al != 0) {
                return false;
            }
            scan = true;
            out->insert(out->end(), seg, seg + 2 + len);
        } else if (marker == 0xDB) {
            // quantisation tables: Pq (8 or 16 bit) and Tq, then 64 entries each
            for (size_t k = 4; k < 2 + len;) {
                const int pq = seg[k] >> 4, tq = seg[k] & 15;
                const size_t size = 1 + 64 * (pq ? 2 : 1);
                if (pq > 1 || tq > 3 || k + size > 2 + len) {
                    return false;
                }
                quantDefined |= 1u << tq;
                k += size;
            }
            out->insert(out->end(), seg, seg + 2 + len);
        } else if (marker == 0xC4) {
            // Huffman tables: Tc and Th, 16 code counts that form a prefix code (no code of all
            // ones, as libjpeg requires), at most 256 values (DC values at most 15)
            for (size_t k = 4; k < 2 + len;) {
                if (k + 17 > 2 + len) {
                    return false;
                }
                const int tc = seg[k] >> 4, th = seg[k] & 15;
                if (tc > 1 || th > 3) {
                    return false;
                }
                size_t total = 0;
                long code = 0;
                for (int l = 1; l <= 16; ++l) {
                    const int count = seg[k + l];
                    total += count;
                    code += count;
                    if (count > 0 && code >= (1L << l)) {
                        return false;
                    }
                    code <<= 1;
                }
                if (total > 256 || k + 17 + total > 2 + len) {
                    return false;
                }
                for (size_t v = 0; tc == 0 && v < total; ++v) {
                    if (seg[k + 17 + v] > 15) {
                        return false;
                    }
                }
                k += 17 + total;
            }
            out->insert(out->end(), seg, seg + 2 + len);
        } else if (marker == 0xDD) {
            // the restart interval
            if (len != 4) {
                return false;
            }
            out->insert(out->end(), seg, seg + 2 + len);
        } else {
            return false; // anything else (other frames, arithmetic coding, DNL, extensions)
        }
        i += 2 + len;
    }
    if (!scan || i != end - 2) {
        return false;
    }
    out->insert(out->end(), { 0xFF, 0xD9 });
    // The colour transform must be the one poppler's DCTStream decodes with: an Adobe marker
    // decides it there (any transform but 0 is YCbCr), but a viewer's libjpeg lets JFIF win
    // over Adobe. So with Adobe, no JFIF: only the APP14 (its transform as 0 or 1); without it,
    // JFIF exactly when libjpeg saw one (else the component ids decide, the same way in both).
    std::vector<unsigned char> app;
    if (adobe) {
        app.insert(app.end(), { 0xFF, 0xEE, 0x00, 0x0E, 'A', 'd', 'o', 'b', 'e', 0x00, 0x64, 0x00, 0x00, 0x00, 0x00, static_cast<unsigned char>(transform ? 1 : 0) });
    } else if (jfif) {
        app.insert(app.end(), { 0xFF, 0xE0, 0x00, 0x10, 'J', 'F', 'I', 'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00 });
    }
    out->insert(out->begin() + static_cast<std::ptrdiff_t>(headerEnd), app.begin(), app.end());
    return true;
}

// Reads at most <limit> bytes of <str> (rewound and closed here) into <out>; false when the
// stream cannot be read or is longer.
static bool readPrefix(Stream *str, size_t limit, std::vector<unsigned char> *out, bool *longer)
{
    out->clear();
    *longer = false;
    if (!str || !str->rewind()) {
        return false;
    }
    std::vector<unsigned char> buf(1 << 16);
    int got;
    while ((got = str->doGetChars(static_cast<int>(std::min<size_t>(buf.size(), limit + 1 - out->size())), buf.data())) > 0) {
        out->insert(out->end(), buf.data(), buf.data() + got);
        if (out->size() > limit) {
            out->resize(limit);
            *longer = true;
            break;
        }
    }
    str->close();
    return true;
}

static uint32_t be32(const unsigned char *p)
{
    return (static_cast<uint32_t>(p[0]) << 24) | (static_cast<uint32_t>(p[1]) << 16) | (static_cast<uint32_t>(p[2]) << 8) | p[3];
}

// The size a JPEG 2000 stream really decodes to: its codestream's SIZ marker (image size minus
// offset, and the component count), read from a raw codestream or from the jp2c box of a JP2
// file. Boxes before jp2c are skipped (at most <data>, the first bytes of the stream). False when
// no SIZ is found or it is malformed.
static bool jpxSize(const std::vector<unsigned char> &data, uint64_t *width, uint64_t *height, int *comps)
{
    const size_t n = data.size();
    size_t at = 0;
    if (n >= 12 && be32(data.data()) == 12 && !memcmp(data.data() + 4, "jP  ", 4)) {
        // JP2 boxes: find the contiguous codestream
        bool found = false;
        int boxes = 0;
        while (at + 8 <= n && ++boxes <= 4096) {
            uint64_t len = be32(data.data() + at);
            size_t header = 8;
            if (len == 1) {
                if (at + 16 > n) {
                    return false;
                }
                len = (static_cast<uint64_t>(be32(data.data() + at + 8)) << 32) | be32(data.data() + at + 12);
                header = 16;
            }
            if (!memcmp(data.data() + at + 4, "jp2c", 4)) {
                at += header;
                found = true;
                break;
            }
            if (len < header || len > n - at) {
                return false; // to the end of the file, or past what was read
            }
            at += static_cast<size_t>(len);
        }
        if (!found) {
            return false;
        }
    }
    // SOC, then SIZ: Lsiz Rsiz Xsiz Ysiz XOsiz YOsiz XTsiz YTsiz XTOsiz YTOsiz Csiz
    if (at + 42 > n || data[at] != 0xFF || data[at + 1] != 0x4F || data[at + 2] != 0xFF || data[at + 3] != 0x51) {
        return false;
    }
    const unsigned char *siz = data.data() + at + 4;
    const uint32_t xsiz = be32(siz + 4), ysiz = be32(siz + 8), xo = be32(siz + 12), yo = be32(siz + 16);
    const int csiz = (siz[36] << 8) | siz[37];
    if (xo >= xsiz || yo >= ysiz || csiz < 1) {
        return false;
    }
    *width = xsiz - xo;
    *height = ysiz - yo;
    *comps = csiz;
    return true;
}

// The segments of an embedded JBIG2 stream (PDF form: segments without the file header), or of
// its JBIG2Globals. Every segment header is read, to the end: the page information segment gives
// the page size (the first one, *width and *height, when pageInfo is set), and every region
// segment (generic, refinement, text, halftone: immediate or intermediate) must fit in
// maxW x maxH, and a pattern dictionary's collective bitmap in maxW * maxH pixels. The page
// size alone does not bound the decode: a 1000x1000 page can carry a 60000x60000 generic region
// that the decoder allocates and fills. False when a header is cut short, a segment's length is
// unknown (0xffffffff), or a size does not fit (and, with pageInfo, when there is no page
// information or its height is unknown).
static bool jbig2Segments(const std::vector<unsigned char> &d, bool pageInfo, uint64_t maxW, uint64_t maxH, uint64_t *width, uint64_t *height)
{
    const size_t n = d.size();
    size_t at = 0;
    bool sized = false;
    while (at < n) {
        if (at + 11 > n) {
            break; // padding: too short for a segment header, so nothing the decoder reads
        }
        const uint32_t number = be32(d.data() + at);
        const unsigned char flags = d[at + 4];
        const int type = flags & 0x3f;
        size_t p = at + 5;
        // referred-to segments: a count in the top 3 bits, or the long form
        uint64_t refs = d[p] >> 5;
        if (refs == 7) {
            if (p + 4 > n) {
                return false;
            }
            refs = be32(d.data() + p) & 0x1fffffff;
            p += 4 + (refs + 8) / 8;
        } else if (refs > 4) {
            return false; // 5 and 6 are invalid counts
        } else {
            p += 1;
        }
        const size_t refSize = number <= 256 ? 1 : number <= 65536 ? 2 : 4;
        if (refs > 4096) {
            return false;
        }
        p += static_cast<size_t>(refs) * refSize;
        p += (flags & 0x40) ? 4 : 1; // page association
        if (p + 4 > n) {
            return false;
        }
        const uint32_t length = be32(d.data() + p);
        p += 4;
        if (length == 0xffffffffu || length > n - p) {
            return false;
        }
        switch (type) {
        case 48: // page information
            if (length < 8) {
                return false;
            }
            if (pageInfo && !sized) {
                const uint32_t w = be32(d.data() + p), h = be32(d.data() + p + 4);
                if (w == 0 || h == 0 || h == 0xffffffffu) {
                    return false;
                }
                *width = w;
                *height = h;
                sized = true;
                maxW = std::min<uint64_t>(maxW, w);
                maxH = std::min<uint64_t>(maxH, h);
            }
            break;
        case 4: // text regions
        case 6:
        case 7:
        case 20: // halftone regions
        case 22:
        case 23:
        case 36: // generic regions
        case 38:
        case 39:
        case 40: // generic refinement regions
        case 42:
        case 43: {
            // region segment information: width, height, x, y, flags
            if (length < 17 || (pageInfo && !sized)) {
                return false;
            }
            if (be32(d.data() + p) > maxW || be32(d.data() + p + 4) > maxH) {
                return false;
            }
            break;
        }
        case 16: { // pattern dictionary: flags, HDPW, HDPH, GRAYMAX
            if (length < 7) {
                return false;
            }
            const uint64_t pw = d[p + 1], ph = d[p + 2], patterns = uint64_t(be32(d.data() + p + 3)) + 1;
            if (static_cast<double>(patterns) * pw * ph > static_cast<double>(maxW) * static_cast<double>(maxH)) {
                return false;
            }
            break;
        }
        default:
            break;
        }
        at = p + length;
    }
    return !pageInfo || sized;
}

// A copy job: the image XObject job.ref, which must be named in the page's resources (directly
// or through forms) and whose dictionary must still give it a copy class (classifyImageCopy,
// as pdftotext -images says copy=), answered without rendering:
//  Jpeg     kind=jpeg: the stream's bytes as stored, unchanged (checked by checkJpeg; at most
//           -max-copy-bytes), width and height its size;
//  Bilevel  kind=gray1: its samples decoded once, packed rows of ceil(width/8) bytes, top row
//           first, most significant bit first, 0 black (the PDF's sense, /Decode absent);
//  Decode   kind=gray8 (a one-component grey space) or rgb8: its samples decoded once at
//           native size through its colour space.
// The page must pass the copy allow-list (ScanAuditOutputDev): the image is the only thing that
// paints on it. Bilevel and Decode images at most -max-copy-pixels. Errors: copy (not such an
// image, class Render, a page that does not pass, or a cross-reference table that was rebuilt
// or changed), copysize, pixels, jpeg (the stream is not a JPEG to copy), decode.
void Renderer::copyImage(const Job &job)
{
    XRef *xref = doc->getXRef();
    // A reference resolves to one object body only while the cross-reference table stays as
    // it is: a rebuilt table (at open, or when any fetch meets a broken entry, the page audit's
    // included) may resolve job.ref to another body than the one the page draws. A document
    // whose table was ever rebuilt is never copied, and the table must not change during the
    // job (checked again before the record is written).
    const unsigned int generation = xref->getGeneration();
    auto xrefStable = [&] { return xref->getReconstructionCount() == 0 && xref->getGeneration() == generation; };
    if (!xrefStable()) {
        emitError(job, "copy");
        return;
    }
    Page *page = doc->getPage(job.page);
    std::set<Ref> forms;
    int budget = copyMaxEntries;
    if (!page || !xobjectReachable(xref, page->getResourceDict(), job.ref, &forms, &budget)) {
        emitError(job, "copy");
        return;
    }
    // the page shows exactly that image: nothing else paints, it is drawn plainly, once. The
    // audit comes first, so the object fetched below is the one it saw drawn (unless the table
    // changed, which refuses the job)
    if (!pageCopyable(job.page, job.ref) || !xrefStable()) {
        emitError(job, "copy");
        return;
    }
    Object image = xref->fetch(job.ref);
    if (!image.isStream() || !image.getStream()->getDict()->lookup("Subtype").isName("Image")) {
        emitError(job, "copy");
        return;
    }
    Stream *str = image.getStream();
    Dict *dict = str->getDict();
    const ImageDictInfo info = Gfx::readImageDictInfo(dict, false, nullptr, &imageInfoCache);
    auto size = [dict](const char *key, const char *abbrev) {
        Object o = dict->lookup(key);
        if (o.isNull()) {
            o = dict->lookup(abbrev);
        }
        return o.isInt() ? o.getInt() : 0;
    };
    const int width = size("Width", "W"), height = size("Height", "H");
    if (info.copy == ImageCopy::Render || width < 1 || height < 1 || !xrefStable()) {
        emitError(job, "copy");
        return;
    }
    if (info.copy == ImageCopy::Jpeg) {
        // the DCT filter's input: the stored (decrypted) bytes
        Stream *raw = str->getKind() == strDCT ? str->getNextStream() : nullptr;
        if (!raw || !raw->rewind()) {
            emitError(job, "jpeg");
            return;
        }
        std::vector<unsigned char> data;
        std::vector<unsigned char> buf(1 << 16);
        bool tooLarge = false;
        int got;
        while ((got = raw->doGetChars(static_cast<int>(buf.size()), buf.data())) > 0) {
            if (data.size() + static_cast<size_t>(got) > maxCopyBytes) {
                tooLarge = true;
                break;
            }
            data.insert(data.end(), buf.data(), buf.data() + got);
        }
        raw->close();
        if (tooLarge) {
            emitError(job, "copysize");
            return;
        }
        const int comps = info.color == ImageColor::Gray || info.color == ImageColor::ICC1 ? 1 : 3;
        std::vector<unsigned char> clean;
        if (!cleanJpeg(data, width, height, comps, &clean)) {
            emitError(job, "jpeg");
            return;
        }
        if (!xrefStable()) {
            emitError(job, "copy");
            return;
        }
        emitRecord(job, "ok", "jpeg", width, height, reinterpret_cast<const char *>(clean.data()), clean.size());
        return;
    }
    // decoded samples: 8 bits (or more) a pixel, but packed 1-bit rows for a bilevel image,
    // whose cap is 8 times the pixels for the same bytes
    const long long pixelCap = info.copy == ImageCopy::Bilevel ? (maxCopyPixels > LLONG_MAX / 8 ? LLONG_MAX : maxCopyPixels * 8) : maxCopyPixels;
    if (static_cast<long long>(width) * height > pixelCap) {
        emitError(job, "pixels");
        return;
    }
    // what the codec really decodes, before it decodes: a JPEG 2000 codestream's SIZ, a JBIG2
    // page information segment, a CCITT stream's /Columns and /Rows, which must be the image's
    // own size (a 100x100 image whose codestream is 16000x16000 would decode it all)
    const StreamKind kind = str->getKind();
    if (kind == strJPX || kind == strJBIG2) {
        std::vector<unsigned char> head;
        bool longer = false;
        uint64_t w = 0, h = 0;
        int ncomps = 1;
        bool sized = readPrefix(str->getNextStream(), maxCopyBytes, &head, &longer);
        if (kind == strJPX) {
            sized = sized && jpxSize(head, &w, &h, &ncomps);
        } else {
            // every segment of the stream and of its globals, read whole (a stream past
            // -max-copy-bytes is refused rather than scanned in part)
            sized = sized && !longer && jbig2Segments(head, true, UINT64_MAX, UINT64_MAX, &w, &h);
            if (sized && w == static_cast<uint64_t>(width) && h == static_cast<uint64_t>(height)) {
                Object parms = dict->lookup("DecodeParms");
                if (parms.isNull()) {
                    parms = dict->lookup("DP");
                }
                if (parms.isArray() && parms.arrayGetLength() == 1) {
                    parms = parms.arrayGet(0);
                }
                Object globals = parms.isDict() ? parms.dictLookup("JBIG2Globals") : Object::null();
                if (globals.isStream()) {
                    std::vector<unsigned char> g;
                    bool gLonger = false;
                    sized = readPrefix(globals.getStream(), maxCopyBytes, &g, &gLonger) && !gLonger && jbig2Segments(g, false, w, h, &w, &h);
                } else if (!globals.isNull()) {
                    sized = false;
                }
            }
        }
        if (!sized) {
            emitError(job, "decode");
            return;
        }
        if (w != static_cast<uint64_t>(width) || h != static_cast<uint64_t>(height) || ncomps > 4) {
            emitError(job, "pixels");
            return;
        }
    } else if (kind == strCCITTFax) {
        Object parms = dict->lookup("DecodeParms");
        if (parms.isNull()) {
            parms = dict->lookup("DP");
        }
        if (parms.isArray() && parms.arrayGetLength() == 1) {
            parms = parms.arrayGet(0);
        }
        const Object columns = parms.isDict() ? parms.dictLookup("Columns") : Object();
        const Object rows = parms.isDict() ? parms.dictLookup("Rows") : Object();
        const int cols = columns.isInt() ? columns.getInt() : 1728;
        if (cols != width || (rows.isInt() && rows.getInt() != height)) {
            emitError(job, "pixels");
            return;
        }
    }
    // a codec under another filter would decode unchecked: refused
    int filters = 0;
    for (Stream *inner = str->getNextStream(); inner; inner = inner->getNextStream()) {
        const StreamKind k = inner->getKind();
        if (k == strJPX || k == strJBIG2 || k == strDCT || k == strCCITTFax || ++filters > 16) {
            emitError(job, "decode");
            return;
        }
    }
    if (info.copy == ImageCopy::Bilevel) {
        // packed rows only from a stream whose bytes are 1-bit rows: a JPEG 2000 or DCT
        // stream decodes to 8 bits a component (render reads it so), never to packed rows
        if (kind == strJPX || kind == strDCT) {
            emitError(job, "copy");
            return;
        }
        const size_t rowBytes = (static_cast<size_t>(width) + 7) / 8;
        std::vector<unsigned char> data(rowBytes * static_cast<size_t>(height), 0xFF);
        if (!str->rewind()) {
            emitError(job, "decode");
            return;
        }
        // a short stream leaves the rest white
        size_t at = 0;
        int got;
        while (at < data.size() && (got = str->doGetChars(static_cast<int>(std::min<size_t>(data.size() - at, 1 << 20)), data.data() + at)) > 0) {
            at += static_cast<size_t>(got);
        }
        str->close();
        if (!xrefStable()) {
            emitError(job, "copy");
            return;
        }
        emitRecord(job, "ok", "gray1", width, height, reinterpret_cast<const char *>(data.data()), data.size());
        return;
    }
    // Decode: the colour space from the dictionary, or a JPX stream's own
    int bits = info.bpc;
    Object csObj = dict->lookup("ColorSpace");
    if (csObj.isNull()) {
        csObj = dict->lookup("CS");
    }
    GfxState state(72.0, 72.0, page->getMediaBox(), 0, true);
    std::unique_ptr<GfxColorSpace> colorSpace;
    if (str->getKind() == strJPX) {
        StreamColorSpaceMode csMode = streamCSNone;
        bool hasAlpha = false;
        str->getImageParams(&bits, &csMode, &hasAlpha);
        if (hasAlpha) {
            emitError(job, "copy");
            return;
        }
        if (csObj.isNull()) {
            if (csMode == streamCSDeviceGray) {
                colorSpace = std::make_unique<GfxDeviceGrayColorSpace>();
            } else if (csMode == streamCSDeviceRGB) {
                colorSpace = std::make_unique<GfxDeviceRGBColorSpace>();
            } else if (csMode == streamCSDeviceCMYK) {
                colorSpace = std::make_unique<GfxDeviceCMYKColorSpace>();
            }
        }
    }
    if (!colorSpace && !csObj.isNull()) {
        colorSpace = GfxColorSpace::parse(nullptr, &csObj, nullptr, &state);
    }
    if (!colorSpace || bits < 1 || bits > 16) {
        emitError(job, "decode");
        return;
    }
    const GfxColorSpaceMode mode = colorSpace->getMode();
    const bool gray = mode == csDeviceGray || mode == csCalGray || (mode == csICCBased && colorSpace->getNComps() == 1);
    Object decode = dict->lookup("Decode");
    if (decode.isNull()) {
        decode = dict->lookup("D");
    }
    GfxImageColorMap colorMap(bits, &decode, std::move(colorSpace));
    if (!colorMap.isOk()) {
        emitError(job, "decode");
        return;
    }
    const int comps = gray ? 1 : 3;
    std::vector<unsigned char> data;
    data.resize(static_cast<size_t>(width) * static_cast<size_t>(height) * comps);
    ImageStream imgStr(str, width, colorMap.getNumPixelComps(), colorMap.getBits());
    if (!imgStr.rewind()) {
        emitError(job, "decode");
        return;
    }
    const size_t rowBytes = static_cast<size_t>(width) * comps;
    for (int y = 0; y < height; ++y) {
        unsigned char *line = imgStr.getLine();
        unsigned char *out = data.data() + static_cast<size_t>(y) * rowBytes;
        if (!line) {
            // a short stream: the rest stays white
            std::fill(out, data.data() + data.size(), 0xFF);
            break;
        }
        if (gray) {
            colorMap.getGrayLine(line, out, width);
        } else {
            colorMap.getRGBLine(line, out, width);
        }
    }
    imgStr.close();
    if (!xrefStable()) {
        emitError(job, "copy");
        return;
    }
    emitRecord(job, "ok", gray ? "gray8" : "rgb8", width, height, reinterpret_cast<const char *>(data.data()), data.size());
}

// cairo's output for one canvas; a discarded canvas drops what cairo writes when it finishes
struct SvgSink
{
    std::string out;
    bool drop = false;
};

static cairo_status_t appendToString(void *closure, const unsigned char *data, unsigned int length)
{
    auto *sink = static_cast<SvgSink *>(closure);
    if (!sink->drop) {
        sink->out.append(reinterpret_cast<const char *>(data), length);
    }
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

// One SVG job's surface and device: the region drawn at the origin.
struct Renderer::SvgCanvas
{
    Region r;
    // heap-held: cairo keeps its address while the canvas moves
    std::unique_ptr<SvgSink> sink = std::make_unique<SvgSink>();
    cairo_surface_t *surface = nullptr;
    cairo_t *cr = nullptr;
    std::unique_ptr<CairoOutputDev> cairo;
};

// Sets up the surface and device; false when the job cannot render.
bool Renderer::beginSvg(SvgCanvas &c)
{
    const Region &r = c.r;
    const double w = r.x1 - r.x0, h = r.y1 - r.y0;
    c.surface = cairo_svg_surface_create_for_stream(appendToString, c.sink.get(), w, h);
    cairo_svg_surface_restrict_to_version(c.surface, CAIRO_SVG_VERSION_1_2);
    cairo_svg_surface_set_document_unit(c.surface, CAIRO_SVG_UNIT_PT);
    if (!cairoFonts) {
        cairo_surface_destroy(c.surface);
        c.surface = nullptr;
        return false;
    }
    c.cairo = noImages ? std::make_unique<NoImages<CairoOutputDev>>() : std::make_unique<CairoOutputDev>();
    c.cairo->startDoc(doc, cairoFonts.get());
    // embedded images are PNG only (pdftomarkdown's SVG allowlist), never the source JPEG etc.
    c.cairo->setEmbedMimeData(false);
    c.cr = cairo_create(c.surface);
    c.cairo->setCairo(c.cr);
    // printing: keep vectors and images at their own resolution
    c.cairo->setPrinting(true);
    cairo_save(c.cr);
    cairo_translate(c.cr, -r.x0, -r.y0);
    cairo_rectangle(c.cr, r.x0, r.y0, w, h);
    cairo_clip(c.cr);
    return true;
}

// Finishes the surface after the page was drawn: nullptr and the SVG, or an
// error code.
const char *Renderer::finishSvg(SvgCanvas &c, std::string *svgOut)
{
    const Region &r = c.r;
    const double w = r.x1 - r.x0, h = r.y1 - r.y0;
    cairo_restore(c.cr);
    c.cairo->setCairo(nullptr);
    c.cairo.reset();
    cairo_status_t status = cairo_status(c.cr);
    cairo_destroy(c.cr);
    c.cr = nullptr;
    cairo_surface_finish(c.surface);
    if (status == CAIRO_STATUS_SUCCESS) {
        status = cairo_surface_status(c.surface);
    }
    cairo_surface_destroy(c.surface);
    c.surface = nullptr;
    if (status != CAIRO_STATUS_SUCCESS) {
        return "render";
    }
    std::string &out = c.sink->out;

    // cairo draws the region at the origin (viewBox "0 0 w h"). Put it back
    // in page coordinates: viewBox = the region, content translated by its
    // origin, so the SVG's user space is the pdftotext word space.
    const size_t svgStart = out.find("<svg");
    const size_t svgEnd = svgStart == std::string::npos ? std::string::npos : out.find('>', svgStart);
    const size_t viewBox = svgStart == std::string::npos ? std::string::npos : out.find(" viewBox=\"", svgStart);
    const size_t close = out.rfind("</svg>");
    if (svgEnd == std::string::npos || viewBox == std::string::npos || viewBox > svgEnd || close == std::string::npos || close < svgEnd) {
        return "render";
    }
    const size_t valueStart = viewBox + 10;
    const size_t valueEnd = out.find('"', valueStart);
    if (valueEnd == std::string::npos || valueEnd > svgEnd) {
        return "render";
    }
    const std::string box = formatNumber(r.x0) + " " + formatNumber(r.y0) + " " + formatNumber(w) + " " + formatNumber(h);
    std::string &svg = *svgOut;
    svg.clear();
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
        std::string().swap(svg);
        return "svgsize";
    }
    return nullptr;
}

void Renderer::emitSvg(const Job &job, const char *error, const std::string &svg)
{
    if (error) {
        emitError(job, error);
    } else {
        emitRecord(job, "ok", "svg", 0, 0, svg.data(), svg.size());
    }
}

void Renderer::renderSvg(const Job &job, const Region &r)
{
    SvgCanvas c;
    c.r = r;
    if (!beginSvg(c)) {
        emitError(job, "render");
        return;
    }
    doc->displayPageSlice(c.cairo.get(), job.page, 72.0, 72.0, 0, true, false, false, -1, -1, -1, -1);
    std::string svg;
    const char *error = finishSvg(c, &svg);
    emitSvg(job, error, svg);
}

// Answer of renderSvgTogether for a job to render on its own.
static const char retryAlone[] = "retry";

// Every SVG job of one page from a single pass over the page (TeeOutputDev).
// False when the pass could differ from separate renders, or met a damaged object or the
// memory limit (results unset): the caller then renders each job on its own. A job whose
// result is not a clean SVG (cairo failed, too large) or does not fit the memory kept for
// later records gets retryAlone, and is rendered on its own when due, so every job's outcome
// is the one it has alone.
bool Renderer::renderSvgTogether(const std::vector<Job> &jobs, const std::vector<std::pair<size_t, Region>> &svgJobs, std::vector<std::pair<const char *, std::string>> *results)
{
    std::vector<SvgCanvas> canvases(svgJobs.size());
    std::vector<OutputDev *> devs;
    bool ok = true;
    for (size_t k = 0; k < svgJobs.size(); ++k) {
        canvases[k].r = svgJobs[k].second;
        if (!beginSvg(canvases[k])) {
            ok = false;
            break;
        }
        devs.push_back(canvases[k].cairo.get());
    }
    const long long start = residentKiB();
    bool unsafe = !ok || start < 0;
    auto fonts = cairoFonts ? cairoFonts->cacheSnapshot() : std::vector<std::shared_ptr<CairoFont>>();
    if (!unsafe) {
        const unsigned long errorsBefore = errorsSeen;
        TeeOutputDev tee(devs, start + svgSlackKiB);
        doc->displayPageSlice(&tee, jobs[svgJobs[0].first].page, 72.0, 72.0, 0, true, false, false, -1, -1, -1, -1, TeeOutputDev::abortCheck, &tee);
        unsafe = tee.isUnsafe() || errorsSeen != errorsBefore;
    }
    results->assign(svgJobs.size(), { nullptr, std::string() });
    size_t kept = 0;
    const size_t keepLimit = static_cast<size_t>(svgSlackKiB) * 1024;
    for (size_t k = 0; k < canvases.size(); ++k) {
        SvgCanvas &c = canvases[k];
        if (!c.cr) {
            continue;
        }
        // cairo decodes images when a surface finishes: past the memory limit the rest are
        // discarded and their jobs drawn alone
        const long long rss = unsafe ? 0 : residentKiB();
        const bool over = !unsafe && (rss < 0 || rss > start + svgSlackKiB);
        if (unsafe || over) {
            if (over) {
                (*results)[k].first = retryAlone;
            }
            // discard: finish and free without keeping anything. A finishing SVG surface replays
            // what was recorded (decoding and encoding every image), so clear the recording first
            // with an unclipped CLEAR paint
            c.sink->drop = true;
            cairo_restore(c.cr);
            c.cairo->setCairo(nullptr);
            c.cairo.reset();
            cairo_destroy(c.cr);
            cairo_t *clear = cairo_create(c.surface);
            cairo_set_operator(clear, CAIRO_OPERATOR_CLEAR);
            cairo_paint(clear);
            cairo_destroy(clear);
            cairo_surface_finish(c.surface);
            cairo_surface_destroy(c.surface);
            continue;
        }
        auto &[error, svg] = (*results)[k];
        error = finishSvg(c, &svg);
        if (error || kept + svg.size() > keepLimit) {
            std::string().swap(svg);
            error = retryAlone;
        } else {
            kept += svg.size();
        }
    }
    if (unsafe && cairoFonts) {
        // the jobs are drawn again one at a time: from the fonts as they were before this pass
        cairoFonts->restoreCache(std::move(fonts));
    }
    return !unsafe;
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

    // SVG jobs of the page are drawn together, up to svgChunk at a time (one pass over the
    // page per chunk), when the first of a chunk is due; records stay in job order.
    // PDFREGIONS_SVG_TOGETHER=0 draws each on its own
    std::vector<std::pair<size_t, Region>> svgJobs;
    for (size_t i = first; i < last; ++i) {
        if (jobs[i].mode == Mode::Svg) {
            if (const std::optional<Region> region = clipToPage(jobs[i], w, h)) {
                svgJobs.emplace_back(i, *region);
            }
        }
    }
    std::vector<std::pair<const char *, std::string>> svgResults;
    size_t nextSvg = 0, chunkStart = 0, chunkEnd = 0;
    bool together = false;

    for (size_t i = first; i < last; ++i) {
        const Job &job = jobs[i];
        if (job.mode == Mode::Copy) {
            copyImage(job);
            continue;
        }
        const std::optional<Region> region = clipToPage(job, w, h);
        if (!region) {
            emitError(job, "empty");
        } else if (job.mode == Mode::Svg) {
            if (nextSvg >= chunkEnd) {
                chunkStart = nextSvg;
                chunkEnd = std::min(svgJobs.size(), nextSvg + svgChunk);
                together = false;
                if (svgTogether && chunkEnd - chunkStart > 1) {
                    const std::vector<std::pair<size_t, Region>> chunk(svgJobs.begin() + chunkStart, svgJobs.begin() + chunkEnd);
                    together = renderSvgTogether(jobs, chunk, &svgResults);
                    ++(together ? svgPagesTogether : svgPagesFallback);
                }
            }
            if (together && svgResults[nextSvg - chunkStart].first == retryAlone) {
                ++svgRetries;
                renderSvg(job, *region);
            } else if (together) {
                auto &[error, svg] = svgResults[nextSvg - chunkStart];
                emitSvg(job, error, svg);
                std::string().swap(svg);
            } else {
                renderSvg(job, *region);
            }
            ++nextSvg;
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
    closeInheritedDescriptors();

    long long maxMemory = 0, maxPixels = 0, timeout = 0, maxSvgBytes = 0, maxOutput = 0, maxCopyBytes = 0, maxCopyPixels = 0;
    const bool ok = parseArgs(argDesc, &argc, argv);
    const bool confineOk = strcmp(confineArg, "require") == 0 || strcmp(confineArg, "auto") == 0 || strcmp(confineArg, "off") == 0;
    const bool limitsOk = confineOk && parseLimit(maxMemoryArg, 0, 1LL << 30, &maxMemory) && parseLimit(maxPixelsArg, 1, 1LL << 30, &maxPixels) && parseLimit(timeoutArg, 0, 86400, &timeout)
            && parseLimit(maxSvgBytesArg, 1, 1LL << 40, &maxSvgBytes) && parseLimit(maxOutputBytesArg, 1, 1LL << 50, &maxOutput) && parseLimit(maxCopyBytesArg, 1, 1LL << 32, &maxCopyBytes)
            && parseLimit(maxCopyPixelsArg, 1, 1LL << 32, &maxCopyPixels);
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

    if (const char *t = getenv("PDFREGIONS_SVG_TOGETHER")) {
        svgTogether = strcmp(t, "0") != 0;
    }
    if (maxMemory > 0) {
        svgSlackKiB = std::min(svgSlackKiB, maxMemory * 1024 / 16);
    }
#ifndef _WIN32
    statmFd = open("/proc/self/statm", O_RDONLY | O_CLOEXEC);
#endif
    if (residentKiB() <= 0) {
        // without a measure of memory a shared pass could not be bounded
        svgTogether = false;
    }
    if (const char *t = getenv("PDFREGIONS_SVG_STATS")) {
        svgStats = strcmp(t, "1") == 0;
    }

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
        Renderer renderer(doc.get(), maxPixels, static_cast<size_t>(maxSvgBytes), static_cast<size_t>(maxCopyBytes), maxCopyPixels);
        for (size_t first = 0; first < jobs.size() && !outputFailed;) {
            size_t last = first + 1;
            while (last < jobs.size() && jobs[last].page == jobs[first].page) {
                ++last;
            }
            renderer.renderPage(jobs, first, last);
            first = last;
        }
    }
    if (svgStats) {
        char line[128];
        snprintf(line, sizeof(line), "svg-together chunks=%ld fallback=%ld retry=%ld", svgPagesTogether, svgPagesFallback, svgRetries);
        writeDiagnostic(line);
    }
    if (outputFailed) {
        return fail(exitWriteError, "cannot write output");
    }
    return exitOk;
}
