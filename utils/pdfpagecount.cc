//========================================================================
//
// pdfpagecount.cc
//
// Count the pages of a PDF by walking its page tree.
//
// This file is licensed under the GPLv2 or later
//
// Copyright (C) 2026 DocSlurp
//
//========================================================================

// The count comes only from structure: the leaf page objects reachable
// from the catalog's /Pages root. It never reads /Count (except to print
// it under -claimed), the linearization dictionary, the Info dictionary,
// XMP metadata or page content, so a document cannot claim its own page
// count. Diagnostics are fixed strings and never echo document data.

#include "config.h"
#include <poppler-config.h>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <map>
#include <memory>
#include <new>
#include <optional>
#include <set>
#include <string>
#include <vector>
#ifndef _WIN32
#    include <unistd.h>
#endif
#include "parseargs.h"
#include "goo/GooString.h"
#include "GlobalParams.h"
#include "Object.h"
#include "Array.h"
#include "Dict.h"
#include "XRef.h"
#include "PDFDoc.h"
#include "Error.h"
#include "ErrorCodes.h"
#include "Win32Console.h"

enum ExitCode
{
    exitOk = 0,
    exitOpenError = 1,
    exitEncrypted = 2,
    exitLimit = 3,
    exitMalformed = 4,
    exitUsage = 99,
};

static char maxPagesArg[32] = "100000";
static char maxDepthArg[32] = "256";
static bool printClaimed = false;
static bool printVersion = false;
static bool printHelp = false;

static const ArgDesc argDesc[] = { { .arg = "-max-pages", .kind = argString, .val = maxPagesArg, .size = sizeof(maxPagesArg), .usage = "fail with exit code 3 above this many pages (default 100000)" },
                                   { .arg = "--max-pages", .kind = argString, .val = maxPagesArg, .size = sizeof(maxPagesArg), .usage = "same as -max-pages" },
                                   { .arg = "-max-depth", .kind = argString, .val = maxDepthArg, .size = sizeof(maxDepthArg), .usage = "fail with exit code 3 above this page tree depth (default 256)" },
                                   { .arg = "--max-depth", .kind = argString, .val = maxDepthArg, .size = sizeof(maxDepthArg), .usage = "same as -max-depth" },
                                   { .arg = "-claimed", .kind = argFlag, .val = &printClaimed, .size = 0, .usage = "also print the root /Count claim on a second line (diagnostic only)" },
                                   { .arg = "--claimed", .kind = argFlag, .val = &printClaimed, .size = 0, .usage = "same as -claimed" },
                                   { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
                                   { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   {} };

// Parses a decimal integer in [minValue, maxValue]; anything else is a usage error.
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

// Poppler's own messages can quote document strings, so they are dropped.
// Some Poppler paths write to stderr directly, so on POSIX fd 2 is pointed at
// /dev/null and this tool's fixed messages go to a saved copy of it.
static void quietErrorCallback(ErrorCategory /*category*/, Goffset /*pos*/, const char * /*msg*/) { }

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

static void writeDiagnostic(const char *msg)
{
    const std::string line = std::string("pdfpagecount: ") + msg + "\n";
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

// Allocation failure (a huge /Kids array, say) is a resource limit, reported
// with a fixed message instead of the C++ runtime's abort text.
static void outOfMemory()
{
    writeDiagnostic("out of memory");
    _Exit(exitLimit);
}

namespace {

enum class Walk
{
    Ok,
    Limit,
    Malformed,
};

// Counts leaf pages with the rules Catalog::cacheSubTree uses to build the
// page list that every util renders from:
//  - a kid that is /Type /Page, or a dictionary without /Kids, is a page;
//  - any other dictionary is an intermediate node, whatever its /Type;
//  - a kid that resolves to something other than a dictionary is skipped;
//  - a kid that is not an indirect reference, a page that Page would reject,
//    or an intermediate node whose /Kids is not an array stops Poppler's page
//    list there, so the tree is reported as malformed.
// Each node's subtree is counted once and memoized, so shared subtrees cost
// nothing extra but still count every time they are referenced (Poppler
// renders them every time). A reference back to a node on the current path
// is a cycle and makes the tree malformed; Poppler instead skips that kid.
// /Count is never consulted, while Poppler stops its page list at /Count and
// treats a /Count above the xref size as zero pages.
class PageTreeCounter
{
public:
    PageTreeCounter(XRef *xrefA, uint64_t maxPagesA, int maxDepthA) : xref(xrefA), limit(maxPagesA), depthLimit(maxDepthA), kidBudget(2 * maxPagesA + 64) { }

    Walk countRoot(Ref rootRef, Object &&root, uint64_t *count);

private:
    struct Frame
    {
        Ref ref;
        Object kids;
        int idx;
        uint64_t sum;
    };

    Walk countNode(Ref ref, Object &&node, uint64_t *count);
    bool isRenderablePage(const Object &pageDict);

    static uint64_t add(uint64_t a, uint64_t b, uint64_t cap) { return (a > cap || b > cap - a) ? cap + 1 : a + b; }

    XRef *xref;
    uint64_t limit;
    int depthLimit;
    // Kid entries examined, pages or not; bounds work on trees padded with
    // kids that count nothing.
    uint64_t kidBudget;
    uint64_t kidsSeen = 0;
    std::map<Ref, uint64_t> counted;
};

// The conditions under which Page::Page leaves the page not ok (and
// Catalog::cacheSubTree stops the page list).
bool PageTreeCounter::isRenderablePage(const Object &pageDict)
{
    const Object &contents = pageDict.dictLookupNF("Contents");
    if (!(contents.isRef() || contents.isArray() || contents.isNull())) {
        return false;
    }
    const Object &annots = pageDict.dictLookupNF("Annots");
    if (annots.isArrayOfLengthAtLeast(10000)) {
        return false;
    }
    if (annots.isRef() && xref->fetch(annots.getRef()).isArrayOfLengthAtLeast(10000)) {
        return false;
    }
    return true;
}

Walk PageTreeCounter::countRoot(Ref rootRef, Object &&root, uint64_t *count)
{
    // Catalog::getNumPages makes a /Type /Page root without a numeric /Count
    // the only page. Any other root is walked as an intermediate node.
    if (root.isDict("Page")) {
        if (root.dictLookup("Count").isNum() || !isRenderablePage(root)) {
            return Walk::Malformed;
        }
        *count = 1;
        return limit < 1 ? Walk::Limit : Walk::Ok;
    }
    // Catalog::initPageList climbs /Parent from a root without /Kids, but
    // getNumPages still reads the original object, so Poppler renders no
    // pages there.
    if (!root.dictLookup("Kids").isArray()) {
        return Walk::Malformed;
    }
    return countNode(rootRef, std::move(root), count);
}

Walk PageTreeCounter::countNode(Ref rootRef, Object &&root, uint64_t *count)
{
    std::vector<Frame> stack;
    std::set<Ref> onPath;
    stack.push_back({ rootRef, root.dictLookup("Kids"), 0, 0 });
    onPath.insert(rootRef);

    while (true) {
        Frame &top = stack.back();
        if (!top.kids.isArray()) {
            return Walk::Malformed;
        }
        if (top.idx >= top.kids.arrayGetLength()) {
            const uint64_t sum = top.sum;
            counted[top.ref] = sum;
            onPath.erase(top.ref);
            stack.pop_back();
            if (stack.empty()) {
                *count = sum;
                return Walk::Ok;
            }
            Frame &parent = stack.back();
            parent.sum = add(parent.sum, sum, limit);
            if (parent.sum > limit) {
                return Walk::Limit;
            }
            parent.idx++;
            continue;
        }

        if (++kidsSeen > kidBudget) {
            return Walk::Limit;
        }
        const Object &kidRef = top.kids.getArray()->getNF(top.idx);
        if (!kidRef.isRef()) {
            return Walk::Malformed;
        }
        const Ref ref = kidRef.getRef();
        if (onPath.contains(ref)) {
            return Walk::Malformed;
        }

        uint64_t kidCount;
        auto memo = counted.find(ref);
        if (memo != counted.end()) {
            kidCount = memo->second;
        } else {
            Object kid = xref->fetch(ref);
            if (kid.isDict("Page") || (kid.isDict() && !kid.getDict()->hasKey("Kids"))) {
                if (!isRenderablePage(kid)) {
                    return Walk::Malformed;
                }
                kidCount = 1;
                counted[ref] = 1;
            } else if (kid.isDict()) {
                if (static_cast<int>(stack.size()) >= depthLimit) {
                    return Walk::Limit;
                }
                Object kids = kid.dictLookup("Kids");
                onPath.insert(ref);
                stack.push_back({ ref, std::move(kids), 0, 0 });
                continue;
            } else {
                kidCount = 0;
                counted[ref] = 0;
            }
        }

        top.sum = add(top.sum, kidCount, limit);
        if (top.sum > limit) {
            return Walk::Limit;
        }
        top.idx++;
    }
}

struct Result
{
    Walk walk;
    uint64_t count;
    std::optional<long long> claimed;
    const char *message;
};

Result countPages(XRef *xref, uint64_t maxPages, int maxDepth)
{
    Object catalog = xref->getCatalog();
    if (!catalog.isDict()) {
        return { Walk::Malformed, 0, {}, "malformed page tree: no catalog dictionary" };
    }
    const Object &rootRefObj = catalog.dictLookupNF("Pages");
    if (!rootRefObj.isRef() || rootRefObj.getRefNum() < 0 || rootRefObj.getRefNum() >= xref->getNumObjects()) {
        return { Walk::Malformed, 0, {}, "malformed page tree: invalid /Pages reference" };
    }
    const Ref rootRef = rootRefObj.getRef();
    Object root = xref->fetch(rootRef);
    if (!root.isDict()) {
        return { Walk::Malformed, 0, {}, "malformed page tree: /Pages is not a dictionary" };
    }

    Result result { Walk::Ok, 0, {}, nullptr };
    Object countObj = root.dictLookup("Count");
    if (countObj.isInt()) {
        result.claimed = countObj.getInt();
    } else if (countObj.isInt64()) {
        result.claimed = countObj.getInt64();
    }

    PageTreeCounter counter(xref, maxPages, maxDepth);
    result.walk = counter.countRoot(rootRef, std::move(root), &result.count);
    if (result.walk == Walk::Ok && result.count == 0) {
        // Every util reads -l 0 as "all pages", so zero is never a count.
        return { Walk::Malformed, 0, {}, "malformed page tree: no pages" };
    }
    if (result.walk == Walk::Limit) {
        result.message = "page tree exceeds the page or depth limit";
    } else if (result.walk == Walk::Malformed) {
        result.message = "malformed page tree";
    }
    return result;
}

}

int main(int argc, char *argv[])
{
    Win32Console win32Console(&argc, &argv);

    long long maxPages = 0, maxDepth = 0;
    const bool ok = parseArgs(argDesc, &argc, argv);
    const bool limitsOk = parseLimit(maxPagesArg, 0, 1000000000LL, &maxPages) && parseLimit(maxDepthArg, 1, 100000, &maxDepth);
    if (!ok || !limitsOk || argc != 2 || printVersion || printHelp) {
        fprintf(stderr, "pdfpagecount version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdfpagecount", "<PDF-file>", argDesc);
            fprintf(stderr, "Exit codes: 0 ok, 1 open error, 2 encrypted (password required), 3 limit exceeded, 4 malformed page tree, 99 usage.\n");
        }
        if ((printVersion || printHelp) && ok && limitsOk) {
            return exitOk;
        }
        return exitUsage;
    }

    silenceStderr();
    std::set_new_handler(outOfMemory);
    globalParams = std::make_unique<GlobalParams>();
    setErrorCallback(quietErrorCallback);

    // Reconstruction during open is how Poppler repairs damaged files. If a
    // later fetch triggers it mid-walk, objects read before and after may
    // disagree, so the walk is repeated once on the rebuilt xref (XRef only
    // reconstructs once).
    bool reconstructed = false;
    auto onReconstructed = [&reconstructed] { reconstructed = true; };

    // No passwords: an empty user password is tried, as for every util.
    auto doc = std::make_unique<PDFDoc>(std::make_unique<GooString>(argv[1]), std::nullopt, std::nullopt, onReconstructed);
    if (!doc->isOk()) {
        if (doc->getErrorCode() == errEncrypted) {
            return fail(exitEncrypted, "document is encrypted and needs a password");
        }
        return fail(exitOpenError, "cannot open document");
    }

    reconstructed = false;
    Result result = countPages(doc->getXRef(), static_cast<uint64_t>(maxPages), static_cast<int>(maxDepth));
    if (reconstructed) {
        reconstructed = false;
        result = countPages(doc->getXRef(), static_cast<uint64_t>(maxPages), static_cast<int>(maxDepth));
        if (reconstructed) {
            return fail(exitMalformed, "malformed page tree: xref changed during the walk");
        }
    }

    switch (result.walk) {
    case Walk::Ok:
        break;
    case Walk::Limit:
        return fail(exitLimit, result.message);
    case Walk::Malformed:
        return fail(exitMalformed, result.message);
    }

    printf("%llu\n", static_cast<unsigned long long>(result.count));
    if (printClaimed) {
        if (result.claimed) {
            printf("%lld\n", *result.claimed);
        } else {
            printf("none\n");
        }
    }
    if (fflush(stdout) != 0 || ferror(stdout)) {
        return fail(exitOpenError, "cannot write output");
    }
    return exitOk;
}
