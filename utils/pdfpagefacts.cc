//========================================================================
//
// pdfpagefacts.cc
//
// Page facts (schema doppler-page-facts/1) as JSON lines on stdout: one document line, one
// line per page, one run line. No text output, no images, no rendering. See
// pdfpagefacts(1) and docslurp/page-facts/README.md.
//
// This file is licensed under the GPLv2 or later
//
// Copyright (C) 2026 DocSlurp
//
//========================================================================

// The tool reads a hostile document: it writes nothing but stdout, never puts document data
// on stderr (every diagnostic is a fixed string; Poppler's own messages are dropped and fd 2
// points at /dev/null while the document is processed), caps its address space and run time,
// closes descriptors it did not open, and confines itself (no writes, no network, no exec)
// once the file is open and before a byte of it is parsed. Confinement is required unless
// -confine says otherwise.

#include "config.h"
#include <poppler-config.h>
#include <cerrno>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <memory>
#include <new>
#include <optional>
#include <string>
#ifndef _WIN32
#    include <csignal>
#    include <sys/resource.h>
#    include <unistd.h>
#endif
#include "parseargs.h"
#include "goo/GooString.h"
#include "goo/gfile.h"
#include "Stream.h"
#include "GlobalParams.h"
#include "GfxFont.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "TextOutputDev.h"
#include "PageFacts.h"
#include "Error.h"
#include "ErrorCodes.h"
#include "XRef.h"
#include "Win32Console.h"
#include "Confine.h"
#include "doppler_commit.h"

enum ExitCode
{
    exitOk = 0,
    exitOpenError = 1,
    exitLocked = 2,
    exitLimit = 3,
    exitUnconfined = 5,
    exitUsage = 99,
};

static int firstPage = 1;
static int lastPage = 0;
static char levelArg[16] = "basic";
static char includeArg[128] = "";
static char ownerPassword[128] = "\001";
static char userPassword[128] = "\001";
static char maxPagesArg[32] = "100000";
static char maxMemoryArg[32] = "4096";
static char scanMaxWordsArg[32] = "20";
static char scanMinCoverageArg[32] = "0.85";
static char scanLayerOverlapArg[32] = "0.5";
static char confineArg[16] = "require";
static char timeoutArg[32] = "600";
static bool timing = false;
static bool printVersion = false;
static bool printHelp = false;

static const ArgDesc argDesc[] = { { .arg = "-f", .kind = argInt, .val = &firstPage, .size = 0, .usage = "first page" },
                                   { .arg = "-l", .kind = argInt, .val = &lastPage, .size = 0, .usage = "last page" },
                                   { .arg = "-level", .kind = argString, .val = levelArg, .size = sizeof(levelArg), .usage = "basic or full (default basic)" },
                                   { .arg = "-include", .kind = argString, .val = includeArg, .size = sizeof(includeArg), .usage = "families added to basic: fonts,oddtext,images-detail,vectors,forms,annots,ocg,structure" },
                                   { .arg = "-opw", .kind = argString, .val = ownerPassword, .size = sizeof(ownerPassword), .usage = "owner password (for encrypted files)" },
                                   { .arg = "-upw", .kind = argString, .val = userPassword, .size = sizeof(userPassword), .usage = "user password (for encrypted files)" },
                                   { .arg = "-max-pages", .kind = argString, .val = maxPagesArg, .size = sizeof(maxPagesArg), .usage = "stop with exit code 3 after this many pages (default 100000)" },
                                   { .arg = "-max-memory", .kind = argString, .val = maxMemoryArg, .size = sizeof(maxMemoryArg), .usage = "address-space limit in MiB, 0 for none (default 4096)" },
                                   { .arg = "-scan-max-words", .kind = argString, .val = scanMaxWordsArg, .size = sizeof(scanMaxWordsArg), .usage = "a page with more visible words is not a scan (default 20)" },
                                   { .arg = "-scan-min-coverage", .kind = argString, .val = scanMinCoverageArg, .size = sizeof(scanMinCoverageArg), .usage = "share of the page images must cover for a scan (default 0.85)" },
                                   { .arg = "-scan-layer-overlap", .kind = argString, .val = scanLayerOverlapArg, .size = sizeof(scanLayerOverlapArg), .usage = "share of the page a layer over a scan must cover (default 0.5)" },
                                   { .arg = "-confine", .kind = argString, .val = confineArg, .size = sizeof(confineArg), .usage = "no-write, no-network sandbox before the PDF is parsed: require, auto or off (default require)" },
                                   { .arg = "-timeout", .kind = argString, .val = timeoutArg, .size = sizeof(timeoutArg), .usage = "wall-clock limit for the whole run in seconds, 0 for none (default 600; exit code 3)" },
                                   { .arg = "-timing", .kind = argFlag, .val = &timing, .size = 0, .usage = "add factsMs to page lines and totalMs, peakRssKiB to the run line (not deterministic)" },
                                   { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
                                   { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   {} };

// A value that fills its buffer may have been cut short by parseArgs: refused.
static bool parseShareArg(const char *text, size_t bufferSize, double *share)
{
    return strlen(text) + 1 < bufferSize && ScanThresholds::parseShare(text, share);
}

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

static int diagnosticsFd = 2;

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

static void writeDiagnostic(const char *msg)
{
    const std::string line = std::string("pdfpagefacts: ") + msg + "\n";
#ifndef _WIN32
    ssize_t ignored = write(diagnosticsFd, line.c_str(), line.size());
    (void)ignored;
#else
    fputs(line.c_str(), stderr);
#endif
}

static int fail(int code, const char *msg)
{
    writeDiagnostic(msg);
    return code;
}

static void outOfMemory()
{
    writeDiagnostic("out of memory");
#ifndef _WIN32
    _exit(exitLimit);
#else
    exit(exitLimit);
#endif
}

#ifndef _WIN32
// -timeout: SIGALRM ends the run with exit code 3. A line being written gets 2 more seconds,
// so the lines on stdout stay whole; the run then stops after it.
static volatile sig_atomic_t writingLine = 0;
static volatile sig_atomic_t deadlinePassed = 0;

static void timeoutExit()
{
    static const char msg[] = "pdfpagefacts: timeout\n";
    ssize_t ignored = write(diagnosticsFd, msg, sizeof(msg) - 1);
    (void)ignored;
    _exit(exitLimit);
}

static void onTimeout(int /*sig*/)
{
    if (writingLine && !deadlinePassed) {
        deadlinePassed = 1;
        alarm(2);
        return;
    }
    timeoutExit();
}
#endif

// One line per write(2), whole or not at all as far as the reader sees: a killed run leaves
// complete lines only.
static bool writeOut(const std::string &line)
{
#ifndef _WIN32
    writingLine = 1;
    struct Done
    {
        ~Done()
        {
            writingLine = 0;
            if (deadlinePassed) {
                timeoutExit();
            }
        }
    } done_;
    size_t done = 0;
    while (done < line.size()) {
        const ssize_t n = write(1, line.data() + done, line.size() - done);
        if (n < 0 && errno == EINTR) {
            continue;
        }
        if (n <= 0) {
            return false;
        }
        done += static_cast<size_t>(n);
    }
    return true;
#else
    return fwrite(line.data(), 1, line.size(), stdout) == line.size() && fflush(stdout) == 0;
#endif
}

static void factsErrorCallback(ErrorCategory category, Goffset /*pos*/, const char *msg)
{
    PageFacts::noteError(category, msg);
}

static double millisecondsSince(std::chrono::steady_clock::time_point start)
{
    return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
}

int main(int argc, char *argv[])
{
    Win32Console win32Console(&argc, &argv);
    const auto runStart = std::chrono::steady_clock::now();

#ifndef _WIN32
    closeInheritedDescriptors();
#endif
    long long maxPages = 0, maxMemory = 0, scanMaxWords = 0, timeout = 0;
    unsigned families = 0;
    ScanThresholds scanThresholds;
    const bool ok = parseArgs(argDesc, &argc, argv);
    const bool confineOk = !strcmp(confineArg, "require") || !strcmp(confineArg, "auto") || !strcmp(confineArg, "off");
    const bool limitsOk = confineOk && parseLimit(maxPagesArg, 1, 1000000000LL, &maxPages) && parseLimit(maxMemoryArg, 0, 1LL << 30, &maxMemory) && parseLimit(scanMaxWordsArg, 0, 1000000000LL, &scanMaxWords)
            && parseShareArg(scanMinCoverageArg, sizeof(scanMinCoverageArg), &scanThresholds.minCoverage) && parseShareArg(scanLayerOverlapArg, sizeof(scanLayerOverlapArg), &scanThresholds.layerOverlap)
            && parseLimit(timeoutArg, 0, 86400, &timeout) && PageFacts::parseLevel(levelArg, &families) && PageFacts::parseInclude(includeArg, &families);
    if (!ok || !limitsOk || argc != 2 || printVersion || printHelp) {
        fprintf(stderr, "pdfpagefacts version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdfpagefacts", "<PDF-file | ->", argDesc);
            fprintf(stderr,
                    "JSON lines on stdout (schema doppler-page-facts/1). Exit codes: 0 ok, 1 cannot open (or stdout cannot be written), 2 encrypted and needs a password, 3 limit (pages, memory, timeout), 5 -confine require could not "
                    "confine, 99 "
                    "usage.\n");
        }
        if ((printVersion || printHelp) && ok && limitsOk) {
            return exitOk;
        }
        return exitUsage;
    }

    silenceStderr();
#ifndef _WIN32
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
    globalParams = std::make_unique<GlobalParams>();
    globalParams->setErrQuiet(true);
    setErrorCallback(factsErrorCallback);

    PageFacts facts(families);
    scanThresholds.maxVisibleWords = static_cast<size_t>(scanMaxWords);
    facts.setScanThresholds(scanThresholds);
    facts.setTiming(timing);

    std::optional<GooString> ownerPW, userPW;
    if (ownerPassword[0] != '\001') {
        ownerPW = GooString(ownerPassword);
    }
    if (userPassword[0] != '\001') {
        userPW = GooString(userPassword);
    }
    const bool fromStdin = !strcmp(argv[1], "-");
    // open the file, then confine, then parse: nothing of the document is read before the
    // sandbox is in place (the file outlives the document, which reads through it)
    std::unique_ptr<GooFile> inputFile;
    if (!fromStdin) {
        inputFile = GooFile::open(std::string(argv[1]));
        if (!inputFile) {
            if (!writeOut(facts.failedDocLine(errOpenFile)) || !writeOut(facts.runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), nullptr))) {
                return fail(exitOpenError, "cannot write output");
            }
            return fail(exitOpenError, "cannot open document");
        }
    }
    if (strcmp(confineArg, "off") != 0) {
        // text extraction loads no system fonts: fontconfig is not initialised first
        const Confinement c = confine(false);
        if (!c.confined()) {
            if (!strcmp(confineArg, "require")) {
                return fail(exitUnconfined, "confinement required but not available");
            }
            writeDiagnostic("warning: confinement not available, running unconfined");
        }
    }
    std::unique_ptr<PDFDoc> doc =
            fromStdin ? PDFDocFactory().createPDFDoc(GooString("fd://0"), ownerPW, userPW) : std::make_unique<PDFDoc>(std::make_unique<FileStream>(inputFile.get(), 0, false, inputFile->size(), Object::null()), ownerPW, userPW);
    if (!doc->isOk()) {
        const int code = doc->getErrorCode();
        if (!writeOut(facts.failedDocLine(code)) || !writeOut(facts.runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), nullptr))) {
            return fail(exitOpenError, "cannot write output");
        }
        if (code == errEncrypted) {
            return fail(exitLocked, "document is encrypted and needs a password");
        }
        return fail(exitOpenError, "cannot open document");
    }
    // pages usually share their fonts: parse each once per document
    doc->getXRef()->setFontCache(std::make_shared<GfxFontCache>());

    if (!writeOut(facts.docLine(doc.get(), ownerPW.has_value() || userPW.has_value()))) {
        return fail(exitOpenError, "cannot write output");
    }

    const int numPages = doc->getNumPages();
    const int first = std::max(firstPage, 1);
    const int last = (lastPage < 1 || lastPage > numPages) ? numPages : lastPage;
    if (last < first) {
        writeOut(facts.runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), nullptr));
        // a document without pages is broken; a first page past the last is a usage error
        return numPages < 1 ? fail(exitOpenError, "document has no pages") : fail(exitUsage, "empty page range");
    }
    doc->skipPageCacheBefore(first);

    // the same device and page geometry as pdftotext -bbox-layout, without laying out lines
    TextOutputDev textOut(nullptr, false, 0, false, false, false);
    if (!textOut.isOk()) {
        return fail(exitOpenError, "cannot set up text extraction");
    }
    textOut.setWordsOnly(true);
    textOut.setPageFacts(&facts);

    const char *stop = nullptr;
    long long pages = 0;
    for (int page = first; page <= last; ++page) {
        if (++pages > maxPages) {
            stop = "max-pages";
            break;
        }
        const auto pageStart = std::chrono::steady_clock::now();
        facts.beginPage(page);
        doc->displayPage(&textOut, page, 72, 72, 0, true, false, false);
        if (!writeOut(facts.finishPage(page, millisecondsSince(pageStart)))) {
            return fail(exitOpenError, "cannot write output");
        }
    }
    if (!writeOut(facts.runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), stop))) {
        return fail(exitOpenError, "cannot write output");
    }
    if (stop) {
        return fail(exitLimit, "page limit reached");
    }
    return exitOk;
}
