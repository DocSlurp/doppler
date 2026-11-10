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
#include "Win32Console.h"

enum ExitCode
{
    exitOk = 0,
    exitOpenError = 1,
    exitUsage = 2,
    exitLimit = 3,
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

static const ArgDesc argDesc[] = { { .arg = "-opw", .kind = argString, .val = ownerPassword, .size = sizeof(ownerPassword), .usage = "owner password (for encrypted files)" },
                                   { .arg = "-upw", .kind = argString, .val = userPassword, .size = sizeof(userPassword), .usage = "user password (for encrypted files)" },
                                   { .arg = "-max-memory", .kind = argString, .val = maxMemoryArg, .size = sizeof(maxMemoryArg), .usage = "address-space limit in MiB, 0 for none (default 2048)" },
                                   { .arg = "-max-pixels", .kind = argString, .val = maxPixelsArg, .size = sizeof(maxPixelsArg), .usage = "largest raster crop in pixels (default 16777216)" },
                                   { .arg = "-timeout", .kind = argString, .val = timeoutArg, .size = sizeof(timeoutArg), .usage = "wall-clock limit for the whole run in seconds, 0 for none (default 120)" },
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
// Records are written with SIGALRM blocked, so when the timeout fires every
// record on stdout is complete; the handler only reports and exits.
static void onTimeout(int /*sig*/)
{
    static const char msg[] = "pdfregions: timeout\n";
    ssize_t ignored = write(diagnosticsFd, msg, sizeof(msg) - 1);
    (void)ignored;
    _exit(exitLimit);
}

static void blockAlarm(bool block)
{
    sigset_t set;
    sigemptyset(&set);
    sigaddset(&set, SIGALRM);
    sigprocmask(block ? SIG_BLOCK : SIG_UNBLOCK, &set, nullptr);
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

static void emitRecord(const Job &job, const char *status, const char *kind, int width, int height, const char *payload, size_t bytes)
{
    if (outputFailed) {
        return;
    }
    const std::string header = job.id + "\t" + status + "\t" + kind + "\t" + std::to_string(width) + "\t" + std::to_string(height) + "\t" + std::to_string(bytes) + "\n";
#ifndef _WIN32
    blockAlarm(true);
    outputFailed = !writeAll(1, header.data(), header.size()) || (bytes > 0 && !writeAll(1, payload, bytes));
    blockAlarm(false);
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

// Device pixels covering a region at dpi: floor of the low edge, ceil of the
// high edge (a hair of tolerance absorbs float noise), cut to the page bitmap.
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
    long long px1 = std::min(static_cast<long long>(std::ceil(r.x1 * s - eps)), pageWpx);
    long long py1 = std::min(static_cast<long long>(std::ceil(r.y1 * s - eps)), pageHpx);
    px0 = std::clamp(px0, 0LL, std::max(pageWpx - 1, 0LL));
    py0 = std::clamp(py0, 0LL, std::max(pageHpx - 1, 0LL));
    px1 = std::max(px1, px0 + 1);
    py1 = std::max(py1, py0 + 1);
    return { px0, py0, px1 - px0, py1 - py0 };
}

//------------------------------------------------------------------------
// rendering
//------------------------------------------------------------------------

class Renderer
{
public:
    Renderer(PDFDoc *docA, long long maxPixelsA) : doc(docA), maxPixels(maxPixelsA)
    {
        SplashColor white;
        white[0] = white[1] = white[2] = 255;
        splash = std::make_unique<SplashOutputDev>(splashModeRGB8, 4, white, true);
        splash->setFontAntialias(true);
        splash->setVectorAntialias(true);
        splash->startDoc(doc);
        cairo = std::make_unique<CairoOutputDev>();
        cairo->startDoc(doc);
    }

    // Answers every job of one page, in order. jobs[first, last) share a page.
    void renderPage(const std::vector<Job> &jobs, size_t first, size_t last);

private:
    void renderRaster(const Job &job, const PixelRect &rect);
    void renderSvg(const Job &job, const Region &r);

    PDFDoc *doc;
    long long maxPixels;
    std::unique_ptr<SplashOutputDev> splash;
    std::unique_ptr<CairoOutputDev> cairo;
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

    long long maxMemory = 0, maxPixels = 0, timeout = 0;
    const bool ok = parseArgs(argDesc, &argc, argv);
    const bool limitsOk = parseLimit(maxMemoryArg, 0, 1LL << 30, &maxMemory) && parseLimit(maxPixelsArg, 1, 1LL << 30, &maxPixels) && parseLimit(timeoutArg, 0, 86400, &timeout);
    if (!ok || !limitsOk || argc != 2 || printVersion || printHelp) {
        fprintf(stderr, "pdfregions version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdfregions", "<PDF-file>", argDesc);
            fprintf(stderr, "Jobs on stdin, records on stdout; see pdfregions(1). Exit codes: 0 ok, 1 cannot open or decrypt, 2 usage or malformed job, 3 limit (memory, timeout).\n");
        }
        return (printVersion || printHelp) && ok && limitsOk ? exitOk : exitUsage;
    }

    silenceStderr();
#ifndef _WIN32
    signal(SIGPIPE, SIG_IGN);
    if (timeout > 0) {
        struct sigaction sa;
        memset(&sa, 0, sizeof(sa));
        sa.sa_handler = onTimeout;
        sigemptyset(&sa.sa_mask);
        sigaction(SIGALRM, &sa, nullptr);
        alarm(static_cast<unsigned>(timeout));
    }
#endif
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

    {
        Renderer renderer(doc.get(), maxPixels);
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
        return fail(exitOpenError, "cannot write output");
    }
    return exitOk;
}
