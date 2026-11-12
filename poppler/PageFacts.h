//========================================================================
//
// PageFacts.h
//
// DocSlurp: per-page facts for routing pages (text layer, images, scans, vectors, fonts,
// forms, annotations, optional content) and one document line, computed on the text
// extraction pass and written as JSON lines (schema doppler-page-facts/1, see
// docslurp/page-facts/README.md). pdftotext -page-facts-fd and pdfpagefacts drive the same
// object, so their lines are byte-identical for the same input, range and level.
//
// A PageFacts is a sink: TextOutputDev (setPageFacts) and TextPage call it from callbacks
// they already receive; Gfx counts operators into its GfxOpTally. Nothing here reads image
// data, glyph programs or page text beyond what the text pass already reads, and nothing
// from the document (no text, names, URIs, paths) is written, except the capped and
// sanitised producer and creator strings of the document line.
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#ifndef PAGEFACTS_H
#define PAGEFACTS_H

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "poppler_private_export.h"
#include "CharTypes.h"
#include "Error.h"
#include "PageFactsHooks.h"

class Annot;
class GfxFont;
class GfxState;
class Page;
class PDFDoc;
class TextFontInfo;
class TextPage;
class TextWord;
class XRef;
struct TextImage;

// Scan detection, shared by page facts and pdftotext -images (scan= on <page>): the page's
// images cover at least scanCover of it (a 64x64 occupancy grid of their visible boxes) and at
// most maxWords words are visible (some glyph not render mode 3 and inside the clip).
//  Single   one image covers at least scanCover of the page by itself;
//  Tiled    several images do together (strips, tiles);
//  Layered  Single, but the covering image has a /SMask or /Mask, or another image (a stencil
//           text layer, say) covering at least half the page lies on it; or Tiled with two
//           such half-page images overlapping (mixed raster content).
struct ScanVerdict
{
    enum Kind
    {
        None,
        Single,
        Tiled,
        Layered
    };
    Kind kind = None;
    int image = 0; // 1-based position of the covering image in the page's image list (0: none)
    double unionCoverage = 0; // grid union of all images, 0..1
    double largest = 0; // the largest single image's share of the page, 0..1
    bool fullPageImage = false; // the geometric half of the test, whatever the words
};

// How a glyph the text pass adds is drawn: in render mode 3 (invisible), and whether its centre
// (half its advance along the baseline, 0.3 of the font size up) lies outside the clip. A word
// is invisible when none of its glyphs is visible (TextPage::getInvisibleWordCount).
struct GlyphVisibility
{
    bool invisible = false;
    bool clipped = false;
    double cx = 0, cy = 0; // the centre
};
POPPLER_PRIVATE_EXPORT GlyphVisibility glyphVisibility(const GfxState *state, double x, double y, double dx, double dy, double fontSize, int rot);

// The thresholds of the scan test (classifyScan): the share of the page one image, or the
// union of the images, must cover; the most visible words a scan may carry; and the share of
// the page a second image must cover to count as a layer on the scan (when it overlaps it).
struct POPPLER_PRIVATE_EXPORT ScanThresholds
{
    static constexpr double defaultMinCoverage = 0.85;
    static constexpr size_t defaultMaxVisibleWords = 20;
    static constexpr double defaultLayerOverlap = 0.5;
    double minCoverage = defaultMinCoverage;
    size_t maxVisibleWords = defaultMaxVisibleWords;
    double layerOverlap = defaultLayerOverlap;
    bool isDefault() const { return minCoverage == defaultMinCoverage && maxVisibleWords == defaultMaxVisibleWords && layerOverlap == defaultLayerOverlap; }
    // Parses a share of the page in (0, 1]: the whole string a finite decimal number
    // (locale-independent). false otherwise, and <share> is left alone.
    static bool parseShare(const char *text, double *share);
};

POPPLER_PRIVATE_EXPORT const char *scanKindName(ScanVerdict::Kind kind);
POPPLER_PRIVATE_EXPORT ScanVerdict classifyScan(const std::vector<TextImage> &images, double pageWidth, double pageHeight, size_t visibleWords, const ScanThresholds &thresholds = {});

// A 64x64 occupancy grid over the page: a box marks the cells whose centres it contains, so
// coverage is quantised to 1/64 of a side and a box costs one OR per row it spans.
struct CoverageGrid
{
    static constexpr int size = 64;
    std::array<uint64_t, size> rows {};
    void clear() { rows.fill(0); }
    // false when the box or the page is not finite or empty
    bool mark(double x0, double y0, double x1, double y1, double pageWidth, double pageHeight);
    bool test(double x, double y, double pageWidth, double pageHeight) const;
    int count() const;
    int countAndNot(const CoverageGrid &other) const; // cells set here and not in other
    double share() const { return count() / double(size * size); }
};

class POPPLER_PRIVATE_EXPORT PageFacts
{
public:
    // Families added to the basic level (-page-facts-include); full is all of them.
    enum Family : unsigned
    {
        Fonts = 1u << 0,
        OddText = 1u << 1,
        ImagesDetail = 1u << 2,
        Vectors = 1u << 3,
        Forms = 1u << 4,
        Annots = 1u << 5,
        Ocg = 1u << 6,
        Structure = 1u << 7,
        AllFamilies = (1u << 8) - 1
    };
    static constexpr const char *schemaName = "doppler-page-facts/1";

    // Parses "basic" or "full" into families (0 or AllFamilies).
    static bool parseLevel(const char *text, unsigned *families);
    // Parses a comma-separated list of family names (fonts,oddtext,images-detail,vectors,forms,
    // annots,ocg,structure) into a mask; false for an unknown name.
    static bool parseInclude(const char *text, unsigned *families);

    explicit PageFacts(unsigned families);
    ~PageFacts();
    PageFacts(const PageFacts &) = delete;
    PageFacts &operator=(const PageFacts &) = delete;

    unsigned families() const { return fams; }
    bool wants(unsigned family) const { return (fams & family) != 0; }
    // Not the defaults: the doc line records them (scanThresholds).
    void setScanThresholds(const ScanThresholds &t) { scanThresholds = t; }
    void setTiming(bool on) { timing = on; }

    //----- driver (pdftotext, pdfpagefacts)

    // The document line, after a successful open. <passwordGiven>: a user or owner password
    // was supplied (to tell an empty user password from a required one).
    std::string docLine(PDFDoc *doc, bool passwordGiven);
    // The document line of a document that could not be opened (PDFDoc error code).
    std::string failedDocLine(int errorCode) const;
    // Before displaying page <pageNum>: from here errors are the page's.
    void beginPage(int pageNum);
    // After displaying it: its line (a failed row if the device never ended the page).
    // <ms>: wall time of the page, written only with timing.
    std::string finishPage(int pageNum, double ms);
    // The last line. <stop>: why the run stopped early (nullptr: it did not).
    std::string runLine(const char *version, const char *commit, double totalMs, const char *stop) const;
    int pagesEmitted() const { return emitted; }
    int pagesFailed() const { return failed; }

    // Poppler's error callback while facts are collected: the category (and, for a few fixed
    // message prefixes, the stage) is recorded for the current page. Never the message.
    static void noteError(ErrorCategory category, const char *msg);

    //----- device hooks

    GfxOpTally *opTally() { return needTally ? &tally : nullptr; }
    void startPage(Page *page, double pageWidth, double pageHeight);
    void onGlyph(const TextFontInfo *font, double fontSize, const Unicode *u, int uLen, int rot, bool diagonal, GlyphVisibility visibility);
    void onUnmappedGlyph(const GfxState *state, const TextFontInfo *font, double fontSize);
    void onWord(const TextWord *word);
    void onPath(const GfxState *state, bool filled);
    void beginAnnot(Annot *annot);
    void endAnnot(Annot *annot);
    void endPage(const TextPage &text);
    bool wantsRules() const { return wants(Vectors); }
    bool wantsGraphics() const { return wants(Vectors); }
    bool wantsPaths() const { return wants(Vectors); }
    bool wantsAnnots() const { return wants(Annots | Forms); }

private:
    struct FontSeen
    {
        const GfxFont *font;
        bool type3, embedded, cid, toUnicode, symbolic, ocr, math, vertical;
    };
    struct AnnotDraw
    {
        Annot *annot;
        uint64_t marks; // paint operators plus glyphs drawn by its appearance
        uint64_t glyphs;
    };
    const FontSeen *fontOf(const TextFontInfo *info);
    uint64_t paintMarks() const;
    std::string serializePage(int pageNum, double ms) const;

    unsigned fams;
    bool needTally;
    bool timing = false;
    ScanThresholds scanThresholds;
    GfxOpTally tally;
    int emitted = 0, failed = 0;
    XRef *docXref = nullptr; // the document's, for xref-repaired

    // the current page
    struct State;
    State *st;
};

#endif
