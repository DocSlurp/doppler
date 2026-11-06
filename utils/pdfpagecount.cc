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
#include <cstdint>
#include <cstdio>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <vector>
#include "parseargs.h"
#include "goo/GooString.h"
#include "GlobalParams.h"
#include "Object.h"
#include "Array.h"
#include "Dict.h"
#include "XRef.h"
#include "Page.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
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

static int maxPages = 100000;
static int maxDepth = 256;
static bool printClaimed = false;
static bool printVersion = false;
static bool printHelp = false;

static const ArgDesc argDesc[] = { { .arg = "-max-pages", .kind = argInt, .val = &maxPages, .size = 0, .usage = "fail with exit code 3 above this many pages (default 100000)" },
                                   { .arg = "--max-pages", .kind = argInt, .val = &maxPages, .size = 0, .usage = "same as -max-pages" },
                                   { .arg = "-max-depth", .kind = argInt, .val = &maxDepth, .size = 0, .usage = "fail with exit code 3 above this page tree depth (default 256)" },
                                   { .arg = "--max-depth", .kind = argInt, .val = &maxDepth, .size = 0, .usage = "same as -max-depth" },
                                   { .arg = "-claimed", .kind = argFlag, .val = &printClaimed, .size = 0, .usage = "also print the root /Count claim on a second line (diagnostic only)" },
                                   { .arg = "--claimed", .kind = argFlag, .val = &printClaimed, .size = 0, .usage = "same as -claimed" },
                                   { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
                                   { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
                                   {} };

// Poppler's own messages can quote document strings, so they are dropped.
static void quietErrorCallback(ErrorCategory /*category*/, Goffset /*pos*/, const char * /*msg*/) { }

static int fail(int code, const char *msg)
{
    fprintf(stderr, "pdfpagecount: %s\n", msg);
    return code;
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
//  - a kid that is not an indirect reference, a page that Page rejects, or
//    an intermediate node whose /Kids is not an array stops Poppler's page
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
    PageTreeCounter(PDFDoc *docA, uint64_t maxPagesA, int maxDepthA) : doc(docA), xref(docA->getXRef()), limit(maxPagesA), depthLimit(maxDepthA) { }

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
    bool isRenderablePage(Ref ref, Object &&pageDict);

    static uint64_t add(uint64_t a, uint64_t b, uint64_t cap) { return (a > cap || b > cap - a) ? cap + 1 : a + b; }

    PDFDoc *doc;
    XRef *xref;
    uint64_t limit;
    int depthLimit;
    std::map<Ref, uint64_t> counted;
};

bool PageTreeCounter::isRenderablePage(Ref ref, Object &&pageDict)
{
    Dict *dict = pageDict.getDict();
    Page page(doc, 1, std::move(pageDict), ref, std::make_unique<PageAttrs>(nullptr, dict));
    return page.isOk();
}

Walk PageTreeCounter::countRoot(Ref rootRef, Object &&root, uint64_t *count)
{
    // Like Catalog::initPageList: a root without /Kids but with a /Parent
    // dictionary is a node inside the tree; climb to the real root.
    if (!root.dictLookup("Kids").isArray() && root.dictLookup("Parent").isDict()) {
        std::set<Ref> seen;
        while (root.isDict() && root.dictLookup("Parent").isDict()) {
            if (!seen.insert(rootRef).second) {
                return Walk::Malformed;
            }
            if (static_cast<int>(seen.size()) > depthLimit) {
                return Walk::Limit;
            }
            const Object &parentRef = root.dictLookupNF("Parent");
            if (!parentRef.isRef()) {
                break;
            }
            rootRef = parentRef.getRef();
            root = xref->fetch(rootRef);
        }
        if (!root.isDict()) {
            return Walk::Malformed;
        }
    }

    if (!root.dictLookup("Kids").isArray()) {
        // Catalog::getNumPages accepts a lone /Type /Page as the root only
        // when there is no numeric /Count; otherwise Poppler renders nothing.
        if (root.getDict()->is("Page") && !root.dictLookup("Count").isNum() && isRenderablePage(rootRef, std::move(root))) {
            *count = 1;
            return limit < 1 ? Walk::Limit : Walk::Ok;
        }
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
                if (!isRenderablePage(ref, std::move(kid))) {
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

}

int main(int argc, char *argv[])
{
    Win32Console win32Console(&argc, &argv);

    const bool ok = parseArgs(argDesc, &argc, argv);
    if (!ok || argc != 2 || printVersion || printHelp || maxPages < 0 || maxDepth < 1) {
        fprintf(stderr, "pdfpagecount version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdfpagecount", "<PDF-file>", argDesc);
            fprintf(stderr, "Exit codes: 0 ok, 1 open error, 2 encrypted (password required), 3 limit exceeded, 4 malformed page tree, 99 usage.\n");
        }
        if (printVersion || printHelp) {
            return exitOk;
        }
        return exitUsage;
    }

    std::string fileName(argv[1]);
    if (fileName == "-") {
        fileName = "fd://0";
    }

    globalParams = std::make_unique<GlobalParams>();
    setErrorCallback(quietErrorCallback);

    // No passwords: an empty user password is tried, as for every util.
    auto doc = std::unique_ptr<PDFDoc>(PDFDocFactory().createPDFDoc(GooString(fileName), std::nullopt, std::nullopt));
    if (!doc->isOk()) {
        if (doc->getErrorCode() == errEncrypted) {
            return fail(exitEncrypted, "document is encrypted and needs a password");
        }
        return fail(exitOpenError, "cannot open document");
    }

    XRef *xref = doc->getXRef();
    Object catalog = xref->getCatalog();
    if (!catalog.isDict()) {
        return fail(exitMalformed, "malformed page tree: no catalog dictionary");
    }
    const Object &rootRefObj = catalog.dictLookupNF("Pages");
    if (!rootRefObj.isRef() || rootRefObj.getRefNum() < 0 || rootRefObj.getRefNum() >= xref->getNumObjects()) {
        return fail(exitMalformed, "malformed page tree: invalid /Pages reference");
    }
    const Ref rootRef = rootRefObj.getRef();
    Object root = xref->fetch(rootRef);
    if (!root.isDict()) {
        return fail(exitMalformed, "malformed page tree: /Pages is not a dictionary");
    }

    std::optional<long long> claimed;
    if (printClaimed) {
        Object countObj = root.dictLookup("Count");
        if (countObj.isInt()) {
            claimed = countObj.getInt();
        } else if (countObj.isInt64()) {
            claimed = countObj.getInt64();
        }
    }

    uint64_t count = 0;
    PageTreeCounter counter(doc.get(), static_cast<uint64_t>(maxPages), maxDepth);
    switch (counter.countRoot(rootRef, std::move(root), &count)) {
    case Walk::Ok:
        break;
    case Walk::Limit:
        return fail(exitLimit, "page tree exceeds the page or depth limit");
    case Walk::Malformed:
        return fail(exitMalformed, "malformed page tree");
    }

    printf("%llu\n", static_cast<unsigned long long>(count));
    if (printClaimed) {
        if (claimed) {
            printf("%lld\n", *claimed);
        } else {
            printf("none\n");
        }
    }
    return exitOk;
}
