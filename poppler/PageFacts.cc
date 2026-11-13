//========================================================================
//
// PageFacts.cc
//
// DocSlurp page facts: see PageFacts.h and docslurp/page-facts/README.md.
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#include <config.h>

#include "PageFacts.h"

#include <algorithm>
#include <bit>
#include <charconv>
#include <cmath>
#include <cstring>
#include <limits>
#include <map>
#include <numeric>
#include <set>
#include <string_view>
#ifndef _WIN32
#    include <sys/resource.h>
#endif

#include "Annot.h"
#include "Catalog.h"
#include "Dict.h"
#include "Array.h"
#include "GfxFont.h"
#include "GfxState.h"
#include "Gfx.h"
#include "Link.h"
#include "Object.h"
#include "OptionalContent.h"
#include "Page.h"
#include "PDFDoc.h"
#include "PDFDocEncoding.h"
#include "SecurityHandler.h"
#include "Stream.h"
#include "TextOutputDev.h"
#include "XRef.h"

//------------------------------------------------------------------------
// image dictionary names and copy class
//------------------------------------------------------------------------

const char *imageFilterName(ImageFilter f)
{
    switch (f) {
    case ImageFilter::None:
        return "none";
    case ImageFilter::DCT:
        return "dct";
    case ImageFilter::JPX:
        return "jpx";
    case ImageFilter::JBIG2:
        return "jbig2";
    case ImageFilter::CCITT:
        return "ccitt";
    case ImageFilter::Flate:
        return "flate";
    case ImageFilter::LZW:
        return "lzw";
    case ImageFilter::RunLength:
        return "rl";
    case ImageFilter::Other:
        break;
    }
    return "other";
}

const char *imageColorName(ImageColor c)
{
    switch (c) {
    case ImageColor::None:
        return "none";
    case ImageColor::Gray:
        return "gray";
    case ImageColor::RGB:
        return "rgb";
    case ImageColor::CMYK:
        return "cmyk";
    case ImageColor::Indexed:
        return "indexed";
    case ImageColor::Lab:
        return "lab";
    case ImageColor::ICC1:
        return "icc1";
    case ImageColor::ICC3:
        return "icc3";
    case ImageColor::ICC4:
        return "icc4";
    case ImageColor::Separation:
        return "sep";
    case ImageColor::DeviceN:
        return "devn";
    case ImageColor::Other:
        break;
    }
    return "other";
}

const char *imageCopyName(ImageCopy c)
{
    switch (c) {
    case ImageCopy::Jpeg:
        return "jpeg";
    case ImageCopy::Bilevel:
        return "bilevel";
    case ImageCopy::Decode:
        return "decode";
    case ImageCopy::Render:
        break;
    }
    return "render";
}

ImageCopy classifyImageCopy(const ImageDictInfo &info)
{
    // Default colour spaces replace Device ones under the page's resources, which a copy job
    // does not see: rendered
    // an image Gfx refuses to draw shows nothing: a copy would show its samples
    if (!info.drawable || info.inlineImg || info.softMask || info.mask || info.external || info.defaultSpaces) {
        return ImageCopy::Render;
    }
    // stored samples are copied only in a Device space: an ICC profile (or a CalGray/CalRGB
    // gamma and matrix) can make a viewer show something else entirely (a constant-white curve
    // hides the scan); those are decoded through their colour space instead
    const bool grayOrRgb = (info.color == ImageColor::Gray || info.color == ImageColor::RGB) && !info.calibrated;
    if (info.filter == ImageFilter::DCT && info.filters == 1 && info.bpc == 8 && grayOrRgb && !info.decode && !info.colorTransform && !info.imageMask) {
        return ImageCopy::Jpeg;
    }
    const bool oneBit = info.bpc == 1 || info.imageMask || info.filter == ImageFilter::CCITT || info.filter == ImageFilter::JBIG2;
    const bool grayish = info.imageMask || (info.color == ImageColor::Gray && !info.calibrated) || info.color == ImageColor::None;
    // one bit for certain: an integer /BitsPerComponent 1, or a stencil (/ImageMask true).
    // Never a JPEG 2000 stream: it decodes to 8 bits a component whatever its dictionary says
    // (JPXStream::getImageParams), so its samples go through the Decode path as Splash reads them
    if (info.filter != ImageFilter::JPX && oneBit && grayish && (info.bpc == 1 || (info.imageMask && info.bpc == 0)) && !info.decode) {
        return ImageCopy::Bilevel;
    }
    if (info.filter == ImageFilter::Other || info.imageMask) {
        return ImageCopy::Render;
    }
    const bool bitsOk = info.bpc == 1 || info.bpc == 2 || info.bpc == 4 || info.bpc == 8 || info.bpc == 16 || (info.bpc == 0 && info.filter == ImageFilter::JPX);
    const bool colorOk = info.color != ImageColor::Other && (info.color != ImageColor::None || info.filter == ImageFilter::JPX);
    return bitsOk && colorOk ? ImageCopy::Decode : ImageCopy::Render;
}

//------------------------------------------------------------------------
// coverage grid
//------------------------------------------------------------------------

namespace {

// [first, last] cell indices whose centres lie in [a, b] on an axis of length <len>
bool cellRange(double a, double b, double len, int *first, int *last)
{
    if (!(len > 0) || !std::isfinite(a) || !std::isfinite(b) || !std::isfinite(len)) {
        return false;
    }
    constexpr int n = CoverageGrid::size;
    const double lo = std::ceil(a / len * n - 0.5);
    const double hi = std::floor(b / len * n - 0.5);
    if (!(lo <= hi) || hi < 0 || lo > n - 1) {
        return false;
    }
    *first = static_cast<int>(std::max(lo, 0.0));
    *last = static_cast<int>(std::min(hi, double(n - 1)));
    return *first <= *last;
}

} // namespace

bool CoverageGrid::mark(double x0, double y0, double x1, double y1, double pageWidth, double pageHeight)
{
    int c0, c1, r0, r1;
    if (!cellRange(std::min(x0, x1), std::max(x0, x1), pageWidth, &c0, &c1) || !cellRange(std::min(y0, y1), std::max(y0, y1), pageHeight, &r0, &r1)) {
        return false;
    }
    const uint64_t bits = (c1 - c0 == 63 ? ~uint64_t(0) : ((uint64_t(1) << (c1 - c0 + 1)) - 1)) << c0;
    for (int r = r0; r <= r1; ++r) {
        rows[r] |= bits;
    }
    return true;
}

bool CoverageGrid::test(double x, double y, double pageWidth, double pageHeight) const
{
    if (!(pageWidth > 0) || !(pageHeight > 0) || !std::isfinite(x) || !std::isfinite(y)) {
        return false;
    }
    const int c = static_cast<int>(std::floor(x / pageWidth * size));
    const int r = static_cast<int>(std::floor(y / pageHeight * size));
    if (c < 0 || c >= size || r < 0 || r >= size) {
        return false;
    }
    return (rows[r] >> c) & 1;
}

int CoverageGrid::count() const
{
    int n = 0;
    for (uint64_t r : rows) {
        n += std::popcount(r);
    }
    return n;
}

int CoverageGrid::countAndNot(const CoverageGrid &other) const
{
    int n = 0;
    for (int i = 0; i < size; ++i) {
        n += std::popcount(rows[i] & ~other.rows[i]);
    }
    return n;
}

//------------------------------------------------------------------------
// scan detection
//------------------------------------------------------------------------

const char *scanKindName(ScanVerdict::Kind kind)
{
    switch (kind) {
    case ScanVerdict::Single:
        return "single";
    case ScanVerdict::Tiled:
        return "tiled";
    case ScanVerdict::Layered:
        return "layered";
    case ScanVerdict::None:
        break;
    }
    return "none";
}

bool ScanThresholds::parseShare(const char *text, double *share)
{
    if (!text || !*text) {
        return false;
    }
    const char *end = text + strlen(text);
    double v = 0;
    const auto r = std::from_chars(text, end, v, std::chars_format::fixed);
    if (r.ec != std::errc() || r.ptr != end || !std::isfinite(v) || !(v > 0) || v > 1) {
        return false;
    }
    *share = v;
    return true;
}

ScanVerdict classifyScan(const std::vector<TextImage> &images, double pageWidth, double pageHeight, size_t visibleWords, const ScanThresholds &thresholds)
{
    const double scanCover = thresholds.minCoverage; // share of the page the images must cover
    const double scanLayer = thresholds.layerOverlap; // a second image at least this large lies on the scan
    ScanVerdict v;
    if (images.empty() || !(pageWidth > 0) || !(pageHeight > 0) || !std::isfinite(pageWidth * pageHeight)) {
        return v;
    }
    const double area = pageWidth * pageHeight;
    CoverageGrid grid;
    int largestIndex = -1, firstSizable = -1;
    std::vector<int> halfPage; // images covering at least scanLayer
    for (size_t i = 0; i < images.size(); ++i) {
        const TextImage &im = images[i];
        grid.mark(im.xMin, im.yMin, im.xMax, im.yMax, pageWidth, pageHeight);
        const double share = std::clamp((im.xMax - im.xMin) * (im.yMax - im.yMin) / area, 0.0, 1.0);
        if (!std::isfinite(share)) {
            continue;
        }
        if (share > v.largest) {
            v.largest = share;
            largestIndex = static_cast<int>(i);
        }
        if (firstSizable < 0 && share >= 0.01) {
            firstSizable = static_cast<int>(i);
        }
        if (share >= scanLayer && halfPage.size() < 8) {
            halfPage.push_back(static_cast<int>(i));
        }
    }
    v.unionCoverage = grid.share();
    v.fullPageImage = v.largest >= scanCover || v.unionCoverage >= scanCover;
    if (!v.fullPageImage || visibleWords > thresholds.maxVisibleWords) {
        return v;
    }
    auto overlap = [&](const TextImage &a, const TextImage &b) {
        const double w = std::min(a.xMax, b.xMax) - std::max(a.xMin, b.xMin);
        const double h = std::min(a.yMax, b.yMax) - std::max(a.yMin, b.yMin);
        return w > 0 && h > 0 && w * h >= 0.25 * area;
    };
    if (v.largest >= scanCover) {
        const TextImage &cover = images[largestIndex];
        bool layered = cover.info.softMask || cover.info.mask;
        for (int j : halfPage) {
            layered = layered || (j != largestIndex && overlap(cover, images[j]));
        }
        v.kind = layered ? ScanVerdict::Layered : ScanVerdict::Single;
        v.image = largestIndex + 1;
    } else {
        bool layered = false;
        for (size_t a = 0; a < halfPage.size(); ++a) {
            for (size_t b = a + 1; b < halfPage.size(); ++b) {
                layered = layered || overlap(images[halfPage[a]], images[halfPage[b]]);
            }
        }
        v.kind = layered ? ScanVerdict::Layered : ScanVerdict::Tiled;
        v.image = (firstSizable >= 0 ? firstSizable : largestIndex) + 1;
    }
    return v;
}

//------------------------------------------------------------------------
// JSON output
//------------------------------------------------------------------------

namespace {

// fixed vocabularies; bit i of a mask is name i, and the names are sorted, so a mask prints
// as a sorted list
constexpr const char *warningNames[] = { "bad-content-stream", "broken-image", "internal-error", "io-error",       "limit-hit",     "missing-font", "non-finite",
                                         "out-of-range",       "page-failed",  "syntax-error",   "syntax-warning", "unimplemented", "xref-repaired", "xref-repaired-earlier" };
enum Warning : unsigned
{
    wBadContent,
    wBrokenImage,
    wInternal,
    wIO,
    wLimit,
    wMissingFont,
    wNonFinite,
    wOutOfRange,
    wPageFailed,
    wSyntaxError,
    wSyntaxWarning,
    wUnimplemented,
    wXrefRepaired,
    wXrefRepairedEarlier
};
constexpr const char *featureNames[] = { "acroform-empty",          "acroform-filled", "annotation-freetext", "annotation-highlight", "annotation-other", "annotation-text", "drawing-limit", "embedded-files", "encrypted-empty-password",
                                         "encrypted-user-password", "huge-page",       "javascript-present",  "layered-scan",         "link-internal",    "link-uri",        "no-tounicode",  "ocg-hidden",     "ocg-visible",
                                         "rotated-content",         "rotated-page",    "scan-page",           "type3-font",           "xfa-present" };
enum Feature : unsigned
{
    fAcroformEmpty,
    fAcroformFilled,
    fAnnotFreeText,
    fAnnotHighlight,
    fAnnotOther,
    fAnnotText,
    fDrawingLimit,
    fEmbeddedFiles,
    fEncryptedEmpty,
    fEncryptedUser,
    fHugePage,
    fJavascript,
    fLayeredScan,
    fLinkInternal,
    fLinkUri,
    fNoToUnicode,
    fOcgHidden,
    fOcgVisible,
    fRotatedContent,
    fRotatedPage,
    fScanPage,
    fType3Font,
    fXfaPresent
};
static_assert(sizeof(featureNames) / sizeof(featureNames[0]) == fXfaPresent + 1);
static_assert(sizeof(warningNames) / sizeof(warningNames[0]) == wXrefRepairedEarlier + 1);

constexpr double maxPoint = 1e9; // larger coordinates are clamped (out-of-range)
constexpr double hugePagePt = 14400; // 200 inches: the PDF 1.x page size limit
constexpr double rotatedContentShare = 0.2;
constexpr uint64_t rotatedContentMinChars = 10;
constexpr size_t maxItems = 32; // images and figure clusters listed per page
constexpr size_t maxGrids = 64; // ruled grids listed per page
constexpr size_t maxGridRules = 4096; // horizontal (and vertical) rules a page's grid search takes
constexpr size_t maxGridWork = size_t(1) << 20; // group and column-rule visits of a page's grid search
constexpr double wordInsideSlack = 1.0; // points a word box (or its centre) may pass an image's box and still lie inside
constexpr double thinRuleMaxPt = 3.0; // a grid's thin rule: at most this thick (issue 43, the layout walker's measure)
constexpr double thinRuleMinPt = 8.0; // and at least this long
constexpr double thinRuleTouchPt = 2.5; // rules this close are one line, and lie on (or inside) a grid's box
constexpr double outerSideShare = 0.8; // a box side carries a rule when thin rules cover this share of it
constexpr size_t walkerMaxRules = 5000; // <rule> elements a page the layout walker reads (past them it sees fewer)
constexpr int maxSizeBins = 8192; // half-point word sizes 0 to 4095.5 counted for a page's body size
constexpr size_t maxFonts = 256; // distinct fonts tracked per page
constexpr uint64_t maxPaths = 100000; // painted paths summarised per page
constexpr size_t maxAnnots = 10000; // annotations examined per page
constexpr size_t maxFormObjects = 65536; // distinct indirect form objects fetched per document
constexpr uint64_t maxFormBytes = 64ull << 20; // bytes of string form values fetched per document
constexpr int maxTreeNodes = 10000; // outline items and name-tree nodes walked
constexpr uint64_t maxTreeEntries = 1000000; // name-tree entries counted (embeddedFiles)
constexpr size_t maxStringPoints = 128; // producer and creator code points
constexpr size_t maxXfaBytes = 1 << 20;

class Json
{
public:
    std::string s;
    unsigned *warnings = nullptr;

    void open(char c = '{')
    {
        comma();
        s += c;
        first = true;
    }
    void close(char c = '}')
    {
        s += c;
        first = false;
    }
    void key(const char *k)
    {
        comma();
        s += '"';
        s += k;
        s += "\":";
        first = true; // the value follows without a comma
    }
    void kopen(const char *k, char c = '{')
    {
        key(k);
        open(c);
    }
    void kint(const char *k, uint64_t v)
    {
        key(k);
        integer(v);
    }
    void integer(uint64_t v)
    {
        comma();
        char buf[24];
        auto r = std::to_chars(buf, buf + sizeof(buf), v);
        s.append(buf, r.ptr);
        first = false;
    }
    void kbool(const char *k, bool v)
    {
        key(k);
        s += v ? "true" : "false";
        first = false;
    }
    void kenum(const char *k, const char *v)
    {
        key(k);
        s += '"';
        s += v;
        s += '"';
        first = false;
    }
    void enumItem(const char *v)
    {
        comma();
        s += '"';
        s += v;
        s += '"';
        first = false;
    }
    // ratios: 0..1, 4 decimals
    void kratio(const char *k, double v)
    {
        key(k);
        number(std::clamp(finite(v), 0.0, 1.0), 4);
    }
    // a configured value, exactly: the shortest text that reads back as the same double
    void kexact(const char *k, double v)
    {
        key(k);
        char buf[64];
        const auto r = std::to_chars(buf, buf + sizeof(buf), finite(v));
        if (r.ec == std::errc()) {
            s.append(buf, r.ptr);
        } else {
            s += '0';
        }
        first = false;
    }
    // a scale factor (UserUnit, 0.0001..1e6): 4 decimals, so its smallest value is not 0
    void kunit(const char *k, double v)
    {
        key(k);
        number(std::clamp(finite(v), 0.0001, 1e6), 4);
    }
    // page points and other measures: 2 decimals, |v| <= maxPoint
    void kpt(const char *k, double v)
    {
        key(k);
        point(v);
    }
    // a non-negative measure with 6 decimals (bytes per pixel), at most maxPoint
    void kmeasure(const char *k, double v)
    {
        key(k);
        v = std::max(finite(v), 0.0);
        if (v > maxPoint) {
            v = maxPoint;
            note(wOutOfRange);
        }
        number(v, 6);
    }
    void point(double v)
    {
        comma();
        v = finite(v);
        if (std::fabs(v) > maxPoint) {
            v = v < 0 ? -maxPoint : maxPoint;
            note(wOutOfRange);
        }
        number(v, 2);
    }
    void kbox(const char *k, double x0, double y0, double x1, double y1)
    {
        kopen(k, '[');
        point(x0);
        point(y0);
        point(x1);
        point(y1);
        close(']');
    }
    // a document string, already sanitised to code points; escaped to ASCII
    void kstring(const char *k, const std::vector<Unicode> &text)
    {
        key(k);
        s += '"';
        for (Unicode u : text) {
            if (u >= 0x20 && u < 0x7f && u != '"' && u != '\\' && u != '<' && u != '>' && u != '&') {
                s += static_cast<char>(u);
            } else if (u >= 0x10000) {
                const unsigned v = u - 0x10000;
                escape(0xd800 + (v >> 10));
                escape(0xdc00 + (v & 0x3ff));
            } else {
                escape(u);
            }
        }
        s += '"';
        first = false;
    }
    void kmask(const char *k, unsigned mask, const char *const *names, int n)
    {
        kopen(k, '[');
        for (int i = 0; i < n; ++i) {
            if (mask & (1u << i)) {
                enumItem(names[i]);
            }
        }
        close(']');
    }

private:
    bool first = true;
    void comma()
    {
        if (!first) {
            s += ',';
        }
        first = false;
    }
    void note(unsigned w)
    {
        if (warnings) {
            *warnings |= 1u << w;
        }
    }
    double finite(double v)
    {
        if (!std::isfinite(v)) {
            note(wNonFinite);
            return 0;
        }
        return v;
    }
    void number(double v, int precision)
    {
        char buf[64];
        auto r = std::to_chars(buf, buf + sizeof(buf), v, std::chars_format::fixed, precision);
        char *end = r.ptr;
        if (r.ec != std::errc()) {
            s += '0';
            first = false;
            return;
        }
        if (precision > 0) {
            while (end > buf && end[-1] == '0') {
                --end;
            }
            if (end > buf && end[-1] == '.') {
                --end;
            }
        }
        const std::string_view text(buf, end - buf);
        s += text == "-0" ? "0" : text;
        first = false;
    }
    void escape(unsigned u)
    {
        static const char hex[] = "0123456789abcdef";
        s += "\\u";
        s += hex[(u >> 12) & 15];
        s += hex[(u >> 8) & 15];
        s += hex[(u >> 4) & 15];
        s += hex[u & 15];
    }
};

PageFacts *activeFacts = nullptr;

// A ruled grid: the rule-only stage of pdftomarkdown's ruled-table search (doppler-md
// tables::ruled_tables, with its default parameters), so the facts name the same grids its
// table conversion starts from. Horizontal rules (and the top and bottom edges of shaded row
// boxes) are joined where collinear segments touch, grouped down the page while they overlap
// and lie close, and split where the spacing jumps; the inner vertical rules crossing at least
// half a group's height are its column lines. The text-dependent steps (a caption band that
// splits two tables, header rows above the first rule, columns from word gutters) are not
// taken, so a table ruled only horizontally has one column here.
struct RuleGrid
{
    double x0, y0, x1, y1;
    size_t rows, columns;
};

// <body>: the page's body text size (thresholds scale with it, as in pdftomarkdown). At most
// maxGridRules horizontal and vertical rules are taken (in drawing order), maxGrids grids kept,
// and maxGridWork open-group and column-rule visits made (grouping is O(h x open groups), and
// a hostile page can keep thousands open); *truncated says a cap was hit, and the grids found
// until then are returned.
std::vector<RuleGrid> ruleGrids(const std::vector<TextRule> &rules, double pageW, double pageH, double body, bool *truncated)
{
    // doppler-md Params defaults: rule_min_len, rule_max_lum, rule_row_gap, rule_split
    constexpr double ruleMinLen = 3.0, ruleMaxLum = 0.97, ruleRowGap = 8.0, ruleSplit = 2.5;
    *truncated = false;
    std::vector<RuleGrid> out;
    if (rules.empty() || !std::isfinite(body) || !(body > 0)) {
        return out;
    }
    struct H
    {
        double x0, x1, y;
    };
    struct V
    {
        double x, y0, y1;
    };
    std::vector<H> h;
    std::vector<V> v;
    // pdftotext -rules writes colours as bytes; pdftomarkdown's luminance reads them back
    auto byte = [](double c) { return static_cast<int>(std::clamp(std::isfinite(c) ? c : 0.0, 0.0, 1.0) * 255.0 + 0.5) / 255.0; };
    for (const TextRule &r : rules) {
        if (!std::isfinite(r.xMin) || !std::isfinite(r.yMin) || !std::isfinite(r.xMax) || !std::isfinite(r.yMax)) {
            continue;
        }
        const double w = r.xMax - r.xMin, hh = r.yMax - r.yMin;
        if (r.box) {
            // shaded rows and headers: their top and bottom edges separate rows; skip page backgrounds
            if (w >= 0.5 * body && w < 0.95 * pageW && hh < 0.5 * pageH) {
                if (h.size() + 2 > maxGridRules) {
                    *truncated = true;
                    continue;
                }
                h.push_back({ r.xMin, r.xMax, r.yMin });
                h.push_back({ r.xMin, r.xMax, r.yMax });
            }
        } else if (0.2126 * byte(r.r) + 0.7152 * byte(r.g) + 0.0722 * byte(r.b) > ruleMaxLum) {
            continue;
        } else if (w >= hh && w >= 0.5 * body) {
            if (h.size() >= maxGridRules) {
                *truncated = true;
                continue;
            }
            h.push_back({ r.xMin, r.xMax, 0.5 * (r.yMin + r.yMax) });
        } else if (hh > w && hh >= 0.8 * body) {
            if (v.size() >= maxGridRules) {
                *truncated = true;
                continue;
            }
            v.push_back({ 0.5 * (r.xMin + r.xMax), r.yMin, r.yMax });
        }
    }
    std::ranges::stable_sort(v, [](const V &a, const V &b) { return a.x < b.x; });
    // join collinear segments that touch (cell borders drawn cell by cell)
    std::ranges::stable_sort(h, [](const H &a, const H &b) {
        const double ya = std::nearbyint(a.y), yb = std::nearbyint(b.y);
        return ya < yb || (ya == yb && a.x0 < b.x0);
    });
    std::vector<H> joined;
    for (const H &r : h) {
        if (!joined.empty() && std::fabs(joined.back().y - r.y) <= 1.0 && r.x0 <= joined.back().x1 + 2.0) {
            joined.back().x1 = std::max(joined.back().x1, r.x1);
        } else {
            joined.push_back(r);
        }
    }
    h.clear();
    for (const H &r : joined) {
        if (r.x1 - r.x0 >= ruleMinLen * body) {
            h.push_back(r);
        }
    }
    std::ranges::stable_sort(h, [](const H &a, const H &b) { return a.y < b.y; });
    // group down the page: a rule joins the first open group whose last rule is within the gap
    // and whose span overlaps it by 0.8 of the narrower one
    struct Group
    {
        double x0, x1;
        std::vector<double> ys;
    };
    std::vector<Group> groups;
    std::vector<size_t> open;
    const double gap = ruleRowGap * body;
    size_t work = maxGridWork;
    auto spend = [&](size_t n) {
        if (n > work) {
            *truncated = true;
            return false;
        }
        work -= n;
        return true;
    };
    for (const H &r : h) {
        if (!spend(open.size() + 1)) {
            groups.clear(); // grouping cut short: no grid from a partial grouping
            return out;
        }
        std::optional<size_t> hit;
        std::erase_if(open, [&](size_t gi) {
            const Group &g = groups[gi];
            if (!(r.y - g.ys.back() <= gap)) {
                return true;
            }
            if (!hit) {
                const double ov = std::min(r.x1, g.x1) - std::max(r.x0, g.x0);
                if (ov >= 0.8 * std::min(r.x1 - r.x0, g.x1 - g.x0)) {
                    hit = gi;
                }
            }
            return false;
        });
        if (hit) {
            Group &g = groups[*hit];
            g.ys.push_back(r.y);
            g.x0 = std::min(g.x0, r.x0);
            g.x1 = std::max(g.x1, r.x1);
        } else {
            open.push_back(groups.size());
            groups.push_back({ r.x0, r.x1, { r.y } });
        }
    }
    auto median = [](std::vector<double> m) {
        std::ranges::sort(m);
        const size_t n = m.size();
        return n % 2 ? m[n / 2] : 0.5 * (m[n / 2 - 1] + m[n / 2]);
    };
    auto keep = [&](double x0, double x1, const std::vector<double> &ys) {
        if (ys.size() < 2) {
            return true;
        }
        const double y0 = ys.front(), y1 = ys.back();
        // inner vertical rules across at least half the height, 2 points apart, are column lines
        const auto lo = std::ranges::partition_point(v, [&](const V &r) { return r.x <= x0 + 2.0; });
        const auto hi = std::partition_point(lo, v.end(), [&](const V &r) { return r.x < x1 - 2.0; });
        if (!spend(static_cast<size_t>(hi - lo) + 1)) {
            return false;
        }
        std::vector<double> xs;
        for (auto it = lo; it != hi; ++it) {
            if (std::min(it->y1, y1) - std::max(it->y0, y0) > 0.5 * (y1 - y0)) {
                xs.push_back(std::round(it->x * 10.0) / 10.0);
            }
        }
        std::ranges::sort(xs);
        size_t lines = 0;
        double last = -INFINITY;
        for (double x : xs) {
            if (x - last > 2.0) {
                ++lines;
                last = x;
            }
        }
        const size_t columns = lines + 1;
        // a grid: at least two rows of rules, or one row split by a column line
        if (ys.size() < 3 && columns < 2) {
            return true;
        }
        if (out.size() >= maxGrids) {
            *truncated = true;
            return false;
        }
        out.push_back({ x0, y0, x1, y1, ys.size() - 1, columns });
        return true;
    };
    // split a group where the rule spacing jumps (two tables sharing a width)
    for (const Group &g : groups) {
        std::vector<double> sorted = g.ys;
        std::ranges::sort(sorted);
        std::vector<double> ys;
        for (double y : sorted) {
            if (ys.empty() || y - ys.back() > 1.5) {
                ys.push_back(y);
            }
        }
        std::vector<double> gaps;
        for (size_t i = 1; i < ys.size(); ++i) {
            gaps.push_back(ys[i] - ys[i - 1]);
        }
        const double limit = gaps.empty() ? 0.0 : ruleSplit * median(gaps);
        std::vector<double> cur;
        for (size_t i = 0; i < ys.size(); ++i) {
            if (i > 0 && ys[i] - ys[i - 1] > limit && gaps.size() >= 3) {
                if (!keep(g.x0, g.x1, cur)) {
                    return out;
                }
                cur.clear();
            }
            cur.push_back(ys[i]);
        }
        if (!keep(g.x0, g.x1, cur)) {
            return out;
        }
    }
    return out;
}

// The distinct (row, column) pairs of a page's rules that cross: a horizontal rule at row y
// (its rounded middle) and a vertical one at column x cross when x lies within the horizontal
// rule's span and y within the vertical one's (both grown by 1.5). <ys> and <xs> are the
// sorted distinct rows and columns. The spans of each row (and each column) are merged first,
// so a pair is met at most once by a sweep over x with a Fenwick tree of the rows whose spans
// are open: O((h + v) log(h + v)), not h * v.
size_t countRuleCrossings(const std::vector<const TextRule *> &hs, const std::vector<const TextRule *> &vs, const std::vector<double> &ys, const std::vector<double> &xs)
{
    struct Span
    {
        size_t line; // row (or column) index
        double lo, hi;
    };
    auto merged = [](std::vector<Span> spans) {
        std::ranges::sort(spans, [](const Span &a, const Span &b) { return a.line < b.line || (a.line == b.line && a.lo < b.lo); });
        std::vector<Span> out;
        for (const Span &sp : spans) {
            if (!out.empty() && out.back().line == sp.line && sp.lo <= out.back().hi) {
                out.back().hi = std::max(out.back().hi, sp.hi);
            } else {
                out.push_back(sp);
            }
        }
        return out;
    };
    auto indexOf = [](const std::vector<double> &v, double x) { return static_cast<size_t>(std::lower_bound(v.begin(), v.end(), x) - v.begin()); };
    std::vector<Span> rows, cols;
    for (const TextRule *h : hs) {
        rows.push_back({ indexOf(ys, std::round(0.5 * (h->yMin + h->yMax))), h->xMin - 1.5, h->xMax + 1.5 });
    }
    for (const TextRule *v : vs) {
        cols.push_back({ indexOf(xs, std::round(0.5 * (v->xMin + v->xMax))), v->yMin - 1.5, v->yMax + 1.5 });
    }
    rows = merged(std::move(rows));
    cols = merged(std::move(cols));
    // events along x: a row span opens at lo, the columns are queried at their x, a row span
    // closes after hi (inclusive bounds: open, query, close at equal x)
    struct Event
    {
        double x;
        int kind; // 0 open, 1 query, 2 close
        size_t i;
    };
    std::vector<Event> events;
    events.reserve(2 * rows.size() + cols.size());
    for (size_t i = 0; i < rows.size(); ++i) {
        events.push_back({ rows[i].lo, 0, i });
        events.push_back({ rows[i].hi, 2, i });
    }
    for (size_t i = 0; i < cols.size(); ++i) {
        events.push_back({ xs[cols[i].line], 1, i });
    }
    std::ranges::sort(events, [](const Event &a, const Event &b) { return a.x < b.x || (a.x == b.x && a.kind < b.kind); });
    std::vector<long> tree(ys.size() + 1, 0);
    auto update = [&tree](size_t i, long d) {
        for (++i; i < tree.size(); i += i & (~i + 1)) {
            tree[i] += d;
        }
    };
    auto prefix = [&tree](size_t n) { // open rows among the first n
        long sum = 0;
        for (; n > 0; n -= n & (~n + 1)) {
            sum += tree[n];
        }
        return sum;
    };
    size_t crossings = 0;
    for (const Event &e : events) {
        if (e.kind == 0) {
            update(rows[e.i].line, 1);
        } else if (e.kind == 2) {
            update(rows[e.i].line, -1);
        } else {
            // rows whose y lies in the column span [lo, hi]
            const size_t first = indexOf(ys, cols[e.i].lo);
            const size_t end = static_cast<size_t>(std::upper_bound(ys.begin(), ys.end(), cols[e.i].hi) - ys.begin());
            if (end > first) {
                crossings += static_cast<size_t>(prefix(end) - prefix(first));
            }
        }
    }
    return crossings;
}

// Issue 43: the thin rules of each ruled grid, as a reader that finds ruled grids from
// connected thin rules (the layout walker's ruled_grids) counts them. A thin rule is a rule
// pdftotext -rules writes as <rule> (a stroked line, or a fill at most 1.5 pt thin; never a
// <box>, whatever its colour), at the 0.01 pt the XHTML writes, at most thinRuleMaxPt thick and
// at least thinRuleMinPt long. Rules of the two directions whose boxes come within
// thinRuleTouchPt join one set (union-find over the page, as the walker does); a set with two
// distinct horizontal lines and three vertical ones is a walker grid.
//
// A grid's rules are the thin rules whose centre line lies in its box grown by thinRuleTouchPt
// and whose span overlaps that box. When some of them belong to a set holding both directions,
// the grid stands for the one with the most rules there (on a tie, the one holding the topmost
// horizontal rule): it is counted (connected) only when every rule of that set is in the box and
// no other grid stands for it, and is otherwise left uncounted (known false). When none does, all
// of them are counted (not connected). hRules and vRules count distinct lines (centres more than
// thinRuleTouchPt apart, clustered from the lowest as the walker's _distinct does), outerSides
// the box sides that the counted rules (centre within thinRuleTouchPt of the side, assigned to
// the nearer side) cover for outerSideShare of their length. *unlisted counts the walker grids no
// counted grid stands for.
struct ThinRule
{
    double x0, y0, x1, y1, c; // c: the centre across (y of a horizontal rule, x of a vertical one)
};

struct GridRules
{
    uint64_t hRules = 0, vRules = 0, outerSides = 0;
    bool connected = false, known = false;
};

// <th> sorted by centre y, <tv> by centre x (each at most maxGridRules long). At most
// maxGridWork rule visits (the touch tests and each grid's candidate rules); false when they run
// out, and nothing is then counted.
bool thinRuleGrids(const std::vector<RuleGrid> &grids, const std::vector<ThinRule> &th, const std::vector<ThinRule> &tv, std::vector<GridRules> *out, uint64_t *unlisted)
{
    constexpr double t = thinRuleTouchPt;
    size_t work = maxGridWork;
    auto spend = [&work](size_t n) {
        if (n > work) {
            work = 0;
            return false;
        }
        work -= n;
        return true;
    };
    const size_t nh = th.size(), n = nh + tv.size();
    auto rule = [&](size_t i) -> const ThinRule & { return i < nh ? th[i] : tv[i - nh]; };
    // the walker's meet: the two boxes within t of each other
    auto meet = [](const ThinRule &a, const ThinRule &b) { return a.x0 - t <= b.x1 && b.x0 - t <= a.x1 && a.y0 - t <= b.y1 && b.y0 - t <= a.y1; };
    std::vector<size_t> parent(n);
    std::iota(parent.begin(), parent.end(), size_t(0));
    auto find = [&parent](size_t i) {
        while (parent[i] != i) {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        return i;
    };
    // a vertical rule that can meet horizontal <r> has its centre within t + thinRuleMaxPt / 2 of
    // r's span (plus a margin for the rounding of a centre); meet() decides
    constexpr double reach = t + 0.5 * thinRuleMaxPt + 0.01;
    for (size_t i = 0; i < nh; ++i) {
        const ThinRule &r = th[i];
        auto it = std::ranges::partition_point(tv, [&](const ThinRule &o) { return o.c < r.x0 - reach; });
        for (; it != tv.end() && it->c <= r.x1 + reach; ++it) {
            if (!spend(1)) {
                return false;
            }
            if (meet(r, *it)) {
                const size_t a = find(i), b = find(nh + static_cast<size_t>(it - tv.begin()));
                if (a != b) {
                    parent[std::max(a, b)] = std::min(a, b); // a set's root is its first rule
                }
            }
        }
    }
    // per set (by root): its rules and its distinct horizontal and vertical lines
    std::vector<size_t> root(n), size(n, 0);
    std::vector<uint64_t> hLines(n, 0), vLines(n, 0);
    std::vector<double> last(n, 0);
    for (size_t i = 0; i < n; ++i) {
        root[i] = find(i);
        ++size[root[i]];
        uint64_t &lines = i < nh ? hLines[root[i]] : vLines[root[i]];
        // th and tv are sorted by c, and a set's horizontal rules all come before its vertical ones
        if (lines == 0 || rule(i).c - last[root[i]] > t) {
            ++lines;
            last[root[i]] = rule(i).c;
        }
    }
    auto both = [&](size_t r) { return hLines[r] > 0 && vLines[r] > 0; };
    // the share of [a, b] the union of <spans> covers
    auto covers = [](std::vector<std::pair<double, double>> &spans, double a, double b) {
        if (!(b > a)) {
            return false;
        }
        std::ranges::sort(spans);
        double covered = 0, end = a;
        for (const auto &[s0, s1] : spans) {
            const double lo = std::max(s0, end), hi = std::min(s1, b);
            if (hi > lo) {
                covered += hi - lo;
                end = hi;
            }
        }
        return covered >= outerSideShare * (b - a);
    };
    out->assign(grids.size(), {});
    std::vector<size_t> chosen(grids.size(), n); // the set each grid stands for (n: none)
    for (size_t g = 0; g < grids.size(); ++g) {
        const RuleGrid &grid = grids[g];
        const double bx0 = grid.x0 - t, by0 = grid.y0 - t, bx1 = grid.x1 + t, by1 = grid.y1 + t;
        const auto h0 = std::ranges::partition_point(th, [&](const ThinRule &r) { return r.c < by0; });
        const auto h1 = std::partition_point(h0, th.end(), [&](const ThinRule &r) { return r.c <= by1; });
        const auto v0 = std::ranges::partition_point(tv, [&](const ThinRule &r) { return r.c < bx0; });
        const auto v1 = std::partition_point(v0, tv.end(), [&](const ThinRule &r) { return r.c <= bx1; });
        if (!spend(static_cast<size_t>((h1 - h0) + (v1 - v0)))) {
            return false;
        }
        std::vector<size_t> members; // indices, horizontal ones first, each run sorted by c
        for (auto it = h0; it != h1; ++it) {
            if (it->x1 >= bx0 && it->x0 <= bx1) {
                members.push_back(static_cast<size_t>(it - th.begin()));
            }
        }
        for (auto it = v0; it != v1; ++it) {
            if (it->y1 >= by0 && it->y0 <= by1) {
                members.push_back(nh + static_cast<size_t>(it - tv.begin()));
            }
        }
        // the set with both directions that has the most rules here (on a tie, the lowest root)
        std::vector<size_t> roots;
        for (size_t i : members) {
            if (both(root[i])) {
                roots.push_back(root[i]);
            }
        }
        std::ranges::sort(roots);
        size_t best = n, bestCount = 0;
        for (size_t i = 0; i < roots.size();) {
            size_t k = i;
            while (k < roots.size() && roots[k] == roots[i]) {
                ++k;
            }
            if (k - i > bestCount) {
                best = roots[i];
                bestCount = k - i;
            }
            i = k;
        }
        GridRules &gr = (*out)[g];
        if (best != n) {
            chosen[g] = best;
            gr.connected = true;
            gr.known = bestCount == size[best]; // every rule of the set lies in this grid's box
            std::erase_if(members, [&](size_t i) { return root[i] != best; });
        } else {
            gr.known = true;
        }
        std::array<std::vector<std::pair<double, double>>, 4> sides; // top, bottom, left, right
        double lastC = 0;
        bool horizontal = true;
        uint64_t lines = 0;
        for (size_t i : members) {
            const ThinRule &r = rule(i);
            if ((i < nh) != horizontal) {
                gr.hRules = lines;
                horizontal = false;
                lines = 0;
            }
            if (lines == 0 || r.c - lastC > t) {
                ++lines;
                lastC = r.c;
            }
            const double s0 = horizontal ? grid.y0 : grid.x0, s1 = horizontal ? grid.y1 : grid.x1;
            const double d0 = std::fabs(r.c - s0), d1 = std::fabs(r.c - s1);
            const std::pair<double, double> span = horizontal ? std::pair { r.x0, r.x1 } : std::pair { r.y0, r.y1 };
            if (d0 <= t && d0 <= d1) {
                sides[horizontal ? 0 : 2].push_back(span);
            } else if (d1 <= t) {
                sides[horizontal ? 1 : 3].push_back(span);
            }
        }
        (horizontal ? gr.hRules : gr.vRules) = lines;
        gr.outerSides = covers(sides[0], grid.x0, grid.x1) + covers(sides[1], grid.x0, grid.x1) + covers(sides[2], grid.y0, grid.y1) + covers(sides[3], grid.y0, grid.y1);
    }
    // a set two grids stand for is counted by neither (one walker grid split in two)
    for (size_t g = 0; g < grids.size(); ++g) {
        for (size_t k = g + 1; k < grids.size(); ++k) {
            if (chosen[g] != n && chosen[g] == chosen[k]) {
                (*out)[g].known = (*out)[k].known = false;
            }
        }
    }
    std::vector<bool> described(n, false);
    for (size_t g = 0; g < grids.size(); ++g) {
        if (chosen[g] != n && (*out)[g].known) {
            described[chosen[g]] = true;
        }
    }
    *unlisted = 0;
    for (size_t i = 0; i < n; ++i) {
        *unlisted += root[i] == i && hLines[i] >= 2 && vLines[i] >= 3 && !described[i];
    }
    return true;
}

bool startsWith(const char *s, const char *prefix)
{
    return strncmp(s, prefix, strlen(prefix)) == 0;
}

// a sanitised document string: decoded (UTF-16BE, UTF-8 or PDFDocEncoding), control characters
// dropped, at most maxStringPoints code points
std::vector<Unicode> sanitize(const std::string &raw, bool *truncated)
{
    std::vector<Unicode> out;
    auto push = [&](Unicode u) {
        if (u < 0x20 || (u >= 0x7f && u <= 0x9f) || (u >= 0xd800 && u <= 0xdfff) || u > 0x10ffff || u == 0xfeff) {
            return true;
        }
        if (out.size() >= maxStringPoints) {
            *truncated = true;
            return false;
        }
        out.push_back(u);
        return true;
    };
    const auto *b = reinterpret_cast<const unsigned char *>(raw.data());
    const size_t n = raw.size();
    if (n >= 2 && b[0] == 0xfe && b[1] == 0xff) {
        for (size_t i = 2; i + 1 < n; i += 2) {
            Unicode u = (b[i] << 8) | b[i + 1];
            if (u >= 0xd800 && u < 0xdc00 && i + 3 < n) {
                const Unicode lo = (b[i + 2] << 8) | b[i + 3];
                if (lo >= 0xdc00 && lo < 0xe000) {
                    u = 0x10000 + ((u - 0xd800) << 10) + (lo - 0xdc00);
                    i += 2;
                }
            }
            if (!push(u)) {
                break;
            }
        }
    } else if (n >= 3 && b[0] == 0xef && b[1] == 0xbb && b[2] == 0xbf) {
        size_t i = 3;
        while (i < n) {
            Unicode u = b[i];
            int extra = u < 0x80 ? 0 : (u >> 5) == 6 ? 1 : (u >> 4) == 14 ? 2 : (u >> 3) == 30 ? 3 : -1;
            if (extra < 0 || i + extra >= n + (extra == 0 ? 1 : 0)) {
                ++i;
                continue;
            }
            if (extra > 0) {
                u &= (0x3f >> extra);
                bool ok = i + extra < n;
                for (int k = 1; ok && k <= extra; ++k) {
                    ok = (b[i + k] & 0xc0) == 0x80;
                    u = (u << 6) | (b[i + k] & 0x3f);
                }
                if (!ok) {
                    ++i;
                    continue;
                }
            }
            i += extra + 1;
            if (!push(u)) {
                break;
            }
        }
    } else {
        for (size_t i = 0; i < n; ++i) {
            if (!push(pdfDocEncoding[b[i]])) {
                break;
            }
        }
    }
    return out;
}

// /Lang only when it looks like a BCP 47 tag: 2-8 letters, then up to 4 subtags of 1-8
// letters or digits
std::optional<std::string> langTag(const Object &lang)
{
    if (!lang.isString()) {
        return std::nullopt;
    }
    // the raw bytes are checked, not a sanitised copy: dropping a control character could turn
    // a malformed tag into a valid-looking one the document never had. ASCII, or UTF-16BE whose
    // code units are all ASCII.
    const std::string &raw = lang.getString();
    std::string s;
    const bool utf16 = raw.size() >= 2 && static_cast<unsigned char>(raw[0]) == 0xfe && static_cast<unsigned char>(raw[1]) == 0xff;
    if (utf16) {
        if (raw.size() % 2 != 0) {
            return std::nullopt;
        }
        for (size_t i = 2; i + 1 < raw.size(); i += 2) {
            if (raw[i] != 0) {
                return std::nullopt;
            }
            s += raw[i + 1];
        }
    } else {
        s = raw;
    }
    if (s.empty() || s.size() > 48) {
        return std::nullopt;
    }
    for (char c : s) {
        if (!isalnum(static_cast<unsigned char>(c)) && c != '-') {
            return std::nullopt;
        }
    }
    size_t pos = 0, len = 0;
    while (pos < s.size() && isalpha(static_cast<unsigned char>(s[pos]))) {
        ++pos;
    }
    if (pos < 2 || pos > 8) {
        return std::nullopt;
    }
    int subtags = 0;
    while (pos < s.size()) {
        if (s[pos] != '-' || ++subtags > 4) {
            return std::nullopt;
        }
        ++pos;
        len = 0;
        while (pos < s.size() && isalnum(static_cast<unsigned char>(s[pos]))) {
            ++pos;
            ++len;
        }
        if (len < 1 || len > 8) {
            return std::nullopt;
        }
    }
    return s;
}

// the action dictionary's /S, or empty
std::string actionKind(const Object &action)
{
    if (!action.isDict()) {
        return {};
    }
    Object s = action.dictLookup("S");
    return s.isName() ? s.getNameString() : std::string();
}

// entries of a name tree (Names pairs), walking at most maxTreeNodes nodes and counting at most
// maxTreeEntries. An indirect /Kids or /Names array is read once however many nodes name it
// (a hostile tree can share one large array among thousands of nodes).
uint64_t countNameTree(Object root, bool *truncated)
{
    uint64_t entries = 0;
    int nodes = 0;
    std::vector<Object> stack;
    std::set<Ref> seen;
    stack.push_back(std::move(root));
    while (!stack.empty()) {
        Object node = std::move(stack.back());
        stack.pop_back();
        if (!node.isDict()) {
            continue;
        }
        if (++nodes > maxTreeNodes) {
            *truncated = true;
            break;
        }
        const Object &namesRef = node.dictLookupNF("Names");
        if (!namesRef.isRef() || seen.insert(namesRef.getRef()).second) {
            Object names = node.dictLookup("Names");
            if (names.isArray()) {
                entries += static_cast<uint64_t>(names.arrayGetLength() / 2);
                if (entries >= maxTreeEntries) {
                    *truncated = true;
                    return maxTreeEntries;
                }
            }
        }
        const Object &kidsRef = node.dictLookupNF("Kids");
        if (kidsRef.isRef() && !seen.insert(kidsRef.getRef()).second) {
            continue;
        }
        Object kids = node.dictLookup("Kids");
        if (kids.isArray()) {
            for (int i = 0; i < kids.arrayGetLength() && static_cast<int>(stack.size()) < maxTreeNodes; ++i) {
                const Object &ref = kids.getArray()->getNF(i);
                if (ref.isRef() && !seen.insert(ref.getRef()).second) {
                    continue;
                }
                stack.push_back(kids.arrayGet(i));
            }
        }
    }
    return entries;
}

// outline items (First/Next), at most maxTreeNodes
int countOutline(const Object &outlines, bool *truncated)
{
    if (!outlines.isDict()) {
        return 0;
    }
    int items = 0;
    std::set<Ref> seen;
    std::vector<Object> stack;
    stack.push_back(outlines.dictLookup("First"));
    while (!stack.empty()) {
        Object item = std::move(stack.back());
        stack.pop_back();
        while (item.isDict()) {
            if (items >= maxTreeNodes) {
                *truncated = true;
                return items;
            }
            ++items;
            const Object &firstRef = item.dictLookupNF("First");
            if (!firstRef.isRef() || seen.insert(firstRef.getRef()).second) {
                stack.push_back(item.dictLookup("First"));
            }
            const Object &nextRef = item.dictLookupNF("Next");
            if (nextRef.isRef() && !seen.insert(nextRef.getRef()).second) {
                break;
            }
            item = item.dictLookup("Next");
        }
    }
    return items;
}

// whether a stream has no filter or a single cheap, streaming text filter (Flate, LZW, ASCIIHex,
// ASCII85, RunLength), so that reading it costs no more than its raw bytes plus the decoded bytes
// kept: an image codec (JPX, JBIG2, DCT, CCITT) decodes far more than it is asked for, and has no
// business in an XML packet; and a filter behind another (Flate over Flate, or ASCIIHex over
// Flate output that is all white space) can make the reader decode gigabytes that yield nothing
bool textFiltersOnly(Stream *str)
{
    Dict *dict = str->getDict();
    if (!dict) {
        return false;
    }
    Object filter = dict->lookup("Filter");
    if (filter.isNull()) {
        return dict->lookupNF("F").isNull(); // no data in an external file either
    }
    auto ok = [](const Object &name) {
        static constexpr const char *names[] = { "FlateDecode", "Fl", "LZWDecode", "LZW", "ASCIIHexDecode", "AHx", "ASCII85Decode", "A85", "RunLengthDecode", "RL" };
        if (!name.isName()) {
            return false;
        }
        for (const char *n : names) {
            if (name.isName(n)) {
                return true;
            }
        }
        return false;
    };
    if (filter.isName()) {
        return ok(filter);
    }
    return filter.isArray() && filter.arrayGetLength() == 1 && ok(filter.arrayGet(0));
}

// reads at most <cap> decoded bytes of a stream whose filters are textFiltersOnly (nothing
// otherwise)
std::string readCapped(Object &obj, size_t cap)
{
    std::string out;
    if (!obj.isStream() || !textFiltersOnly(obj.getStream())) {
        return out;
    }
    Stream *str = obj.getStream();
    if (!str->rewind()) {
        return out;
    }
    unsigned char buf[4096];
    while (out.size() < cap) {
        const int want = static_cast<int>(std::min(sizeof(buf), cap - out.size()));
        const int got = str->doGetChars(want, buf);
        if (got <= 0) {
            break;
        }
        out.append(reinterpret_cast<const char *>(buf), got);
    }
    str->close();
    return out;
}

// XFA: dynamic when the config packet sets <dynamicRender>required</dynamicRender>
const char *xfaKind(const Object &xfa)
{
    std::string text;
    if (xfa.isStream()) {
        Object copy = xfa.copy();
        text = readCapped(copy, maxXfaBytes);
    } else if (xfa.isArray()) {
        // each distinct packet stream is read once: a stream that decodes to nothing costs its
        // raw bytes however many times the array names it
        const int n = std::min(xfa.arrayGetLength(), 128);
        std::set<Ref> read;
        for (int i = 0; i + 1 < n && text.size() < maxXfaBytes; i += 2) {
            Object name = xfa.arrayGet(i);
            if (name.isString() && name.getString() == "config") {
                const Object &packetRef = xfa.getArray()->getNF(i + 1);
                if (packetRef.isRef() && !read.insert(packetRef.getRef()).second) {
                    continue;
                }
                Object packet = xfa.arrayGet(i + 1);
                text += readCapped(packet, maxXfaBytes - text.size());
            }
        }
    } else {
        return "none";
    }
    const size_t at = text.find("dynamicRender");
    if (at != std::string::npos) {
        const size_t gt = text.find('>', at);
        if (gt != std::string::npos) {
            size_t i = gt + 1;
            while (i < text.size() && isspace(static_cast<unsigned char>(text[i]))) {
                ++i;
            }
            if (text.compare(i, 8, "required") == 0) {
                return "dynamic";
            }
        }
    }
    return "static";
}

bool pointsDiffer(const PDFRectangle &a, const PDFRectangle &b)
{
    return std::fabs(a.x1 - b.x1) > 0.005 || std::fabs(a.y1 - b.y1) > 0.005 || std::fabs(a.x2 - b.x2) > 0.005 || std::fabs(a.y2 - b.y2) > 0.005;
}

// saturating add
inline void add(uint64_t &a, uint64_t b)
{
    a = b > UINT64_MAX - a ? UINT64_MAX : a + b;
}

// the form objects fetched for a document (FormObjects, below)
struct FormCache
{
    std::map<Ref, Object> objects;
    uint64_t stringBytes = 0;
    bool exhausted = false;
};


// Bar charts drawn only with filled boxes (doppler#38), which the producer writes as box rules and never as graphics. The
// same rule as doppler-md `vector::bar_regions` (the figures pdftomarkdown makes), constant for constant:
// - a run is at least kMinBars boxes whose bases (the bottom edge of vertical bars, the left edge of horizontal ones) are
//   within kBarBaseTolerance points, with gaps of at most kMaxBarGap times the widest bar so far;
// - its lengths differ (kMinBarSpread: shaded rows and zebra stripes are one length);
// - few hold a word's centre (kMaxBarTextShare: shaded cells and highlights hold them);
// - a stroked axis lies along the baseline (within kBarAxisReach points) and spans kBarAxisSpan of the run: a box's own
//   outline or a cell border beside one box is not an axis (form fields);
// - a scale: at least kBarScaleMin numbers beside the run, increasing along its value axis (redactions and blank form
//   fields have none; they come in every colour, and black bar charts are common, so colour does not decide);
// - white fills (backgrounds), boxes over a quarter of the page and the header and footer bands are never bars.
// A run takes in the boxes stacked on it (along its orientation, until nothing changes); runs that overlap are one cluster.
// At most kMaxBarBoxes boxes and kMaxBarRuns runs a page. Bar detection has its own per-page budget (kBarBudget): it never
// spends the document's drawing budget, so no page can blind later pages' getGraphics. Over a cap or the budget, the
// page's limitHit is set and no more comparisons are made.
constexpr size_t kMaxBarBoxes = 4000;
constexpr size_t kMaxBarRuns = 64;
constexpr size_t kMaxBarWords = 100000;
constexpr size_t kMaxScaleNumbers = 20000;
constexpr size_t kBarScaleMin = 3;
constexpr double kBarScaleAcross = 120;
constexpr double kBarScaleAlong = 40;
constexpr size_t kBarBudget = 8000000;
constexpr size_t kMinBars = 3;
constexpr double kMaxBarGap = 3.0;
constexpr double kMinBarSpread = 1.15;
constexpr double kMaxBarTextShare = 0.34;
constexpr double kBarFurnitureBand = 0.09;
constexpr double kBarBaseTolerance = 0.5;
constexpr double kBarAxisReach = 2.0;
constexpr double kBarAxisSpan = 0.8;
constexpr int kBarStackRounds = 4;
constexpr double kBarRuleReach = 12; // points: an axis or tick a little way off the bars is still the chart's

struct BarBox
{
    double x0, y0, x1, y1;
};

struct BarLine
{
    double at, lo, hi;
};

struct WordCentre
{
    double x, y;
};

struct ScaleNumber
{
    double x0, y0, x1, y1, v;
};

// A tick label's number, as doppler-md `scale_number`: an optional sign (-, U+2212 or +), digits with optional thousands
// commas and one decimal point, an optional %; at most 16 characters.
bool scaleNumber(const Unicode *text, int length, double *value)
{
    if (length <= 0 || length > 16) {
        return false;
    }
    int end = length, i = 0;
    if (text[end - 1] == '%') {
        --end;
    }
    bool neg = false;
    if (i < end && (text[i] == '-' || text[i] == 0x2212)) {
        neg = true;
        ++i;
    } else if (i < end && text[i] == '+') {
        ++i;
    }
    char digits[20];
    int n = 0;
    bool dot = false, prevDigit = false, any = false;
    for (; i < end; ++i) {
        const Unicode c = text[i];
        if (c >= '0' && c <= '9') {
            digits[n++] = char(c);
            prevDigit = any = true;
        } else if (c == ',' && prevDigit && !dot) {
            prevDigit = false;
        } else if (c == '.' && !dot) {
            dot = true;
            prevDigit = false;
            digits[n++] = '.';
        } else {
            return false;
        }
    }
    if (!any || !prevDigit) {
        return false;
    }
    digits[n] = 0;
    const double v = std::strtod(digits, nullptr);
    if (!std::isfinite(v)) {
        return false;
    }
    *value = neg ? -v : v;
    return true;
}

bool spend(size_t *budget, size_t n, bool *hit)
{
    if (*budget < n) {
        *budget = 0;
        *hit = true;
        return false;
    }
    *budget -= n;
    return true;
}

// White (a background), on 8-bit channels as doppler-md reads the producer's #rrggbb.
bool barFillExcluded(double r, double g, double b)
{
    auto c = [](double v) { return std::isfinite(v) ? std::lround(std::clamp(v, 0.0, 1.0) * 255) : 0L; };
    return c(r) == 255 && c(g) == 255 && c(b) == 255;
}

// Whether a run (its union `u`) has a scale: kBarScaleMin numbers beside it increasing along its value axis (up for vertical
// bars, from labels left or right of the plot; right for horizontal bars, from labels below or above it). As doppler-md
// `bar_scale`; every number looked at is charged.
bool barScale(const BarBox &u, bool vertical, const std::vector<ScaleNumber> &nums, size_t *budget, bool *hit)
{
    std::vector<std::pair<double, double>> seq;
    for (int side = 0; side < 2; ++side) {
        seq.clear();
        for (const ScaleNumber &n : nums) {
            if (!spend(budget, 1, hit)) {
                return false;
            }
            const double cx = (n.x0 + n.x1) / 2, cy = (n.y0 + n.y1) / 2;
            bool keep;
            if (vertical) {
                keep = cy >= u.y0 - kBarScaleAlong && cy <= u.y1 + 12
                        && (side == 0 ? (n.x1 <= u.x0 + 4 && n.x1 >= u.x0 - kBarScaleAcross) : (n.x0 >= u.x1 - 4 && n.x0 <= u.x1 + kBarScaleAcross));
            } else {
                keep = cx >= u.x0 - 12 && cx <= u.x1 + kBarScaleAlong
                        && (side == 0 ? (n.y0 >= u.y1 - 4 && n.y0 <= u.y1 + kBarScaleAcross) : (n.y1 <= u.y0 + 4 && n.y1 >= u.y0 - kBarScaleAcross));
            }
            if (keep) {
                seq.push_back({ vertical ? -cy : cx, n.v });
            }
        }
        if (seq.size() < kBarScaleMin) {
            continue;
        }
        std::stable_sort(seq.begin(), seq.end(), [](const auto &a, const auto &b) { return a.first < b.first; });
        bool increasing = true;
        for (size_t k = 1; k < seq.size() && increasing; ++k) {
            increasing = seq[k].second > seq[k - 1].second;
        }
        if (increasing) {
            return true;
        }
    }
    return false;
}

BarBox barUnion(const BarBox &a, const BarBox &b)
{
    return { std::min(a.x0, b.x0), std::min(a.y0, b.y0), std::max(a.x1, b.x1), std::max(a.y1, b.y1) };
}

// `words` sorted by x. Returns clusters to add; joins into `drawings` (getGraphics' clusters) any it overlaps.
std::vector<TextGraphic> barClusters(const std::vector<TextRule> &rules, const std::vector<WordCentre> &words, const std::vector<ScaleNumber> &nums, double pageW, double pageH,
                                     std::vector<TextGraphic> &drawings, size_t *runBudget, bool *limitHit)
{
    std::vector<TextGraphic> out;
    if (!(pageW > 0) || !(pageH > 0) || !std::isfinite(pageW * pageH)) {
        return out;
    }
    // this page's budget: kBarBudget, or what the run has left if less; what it spends comes off the run's
    size_t budgetLeft = std::min(kBarBudget, *runBudget);
    size_t *budget = &budgetLeft;
    struct Charge
    {
        size_t start;
        size_t *left, *run;
        ~Charge() { *run -= std::min(*run, start - *left); }
    } charge { budgetLeft, budget, runBudget };
    if (budgetLeft == 0) {
        *limitHit = true;
        return out;
    }
    const double area = pageW * pageH, band = kBarFurnitureBand * pageH;
    std::vector<BarBox> filled;
    std::vector<BarBox> strokes;
    std::vector<BarLine> across, down;
    for (const TextRule &r : rules) {
        if (r.imageMask) {
            continue;
        }
        const BarBox b { r.xMin, r.yMin, r.xMax, r.yMax };
        const double w = b.x1 - b.x0, h = b.y1 - b.y0;
        if (!std::isfinite(w) || !std::isfinite(h)) {
            continue;
        }
        if (!r.box) {
            strokes.push_back(b);
            if (h <= kBarAxisReach && w > h) {
                across.push_back({ (b.y0 + b.y1) / 2, b.x0, b.x1 });
            } else if (w <= kBarAxisReach && h > w) {
                down.push_back({ (b.x0 + b.x1) / 2, b.y0, b.y1 });
            }
            continue;
        }
        if (barFillExcluded(r.r, r.g, r.b) || !(w > 0) || !(h > 0) || w * h >= 0.25 * area || b.y1 <= band || b.y0 >= pageH - band) {
            continue;
        }
        if (filled.size() < kMaxBarBoxes) {
            filled.push_back(b);
        }
    }
    if (filled.size() < kMinBars || !spend(budget, 4 * filled.size() + 2 * (strokes.size() + words.size()) + 1, limitHit)) {
        return out;
    }
    auto byAt = [](const BarLine &a, const BarLine &b) { return a.at < b.at; };
    std::sort(across.begin(), across.end(), byAt);
    std::sort(down.begin(), down.end(), byAt);
    std::vector<std::pair<BarBox, bool>> found;
    bool full = false;
    // the spread first (cheapest), then words, then the axis; every comparison charged
    auto consider = [&](const std::vector<BarBox> &run, bool vertical) {
        if (full || run.size() < kMinBars || !spend(budget, run.size(), limitHit)) {
            return;
        }
        double lo = INFINITY, hi = 0;
        BarBox u { INFINITY, INFINITY, -INFINITY, -INFINITY };
        for (const BarBox &b : run) {
            const double len = vertical ? b.y1 - b.y0 : b.x1 - b.x0;
            lo = std::min(lo, len);
            hi = std::max(hi, len);
            u = barUnion(u, b);
        }
        if (!(lo > 0) || hi < kMinBarSpread * lo) {
            return;
        }
        const size_t limit = size_t(std::floor(kMaxBarTextShare * double(run.size())));
        size_t withText = 0;
        for (const BarBox &b : run) {
            auto it = std::lower_bound(words.begin(), words.end(), b.x0, [](const WordCentre &w, double x) { return w.x < x; });
            bool held = false;
            for (; it != words.end(); ++it) {
                if (!spend(budget, 1, limitHit)) {
                    return;
                }
                if (it->x > b.x1) {
                    break;
                }
                if (b.y0 <= it->y && it->y <= b.y1) {
                    held = true;
                    break;
                }
            }
            if (held && ++withText > limit) {
                return;
            }
        }
        const std::vector<BarLine> &lines = vertical ? across : down;
        const double at = vertical ? u.y1 : u.x0, from = vertical ? u.x0 : u.y0, to = vertical ? u.x1 : u.y1;
        auto first = std::lower_bound(lines.begin(), lines.end(), at - kBarAxisReach, [](const BarLine &l, double v) { return l.at < v; });
        for (auto it = first; it != lines.end() && it->at <= at + kBarAxisReach; ++it) {
            if (!spend(budget, 1, limitHit)) {
                return;
            }
            if (std::min(it->hi, to) - std::max(it->lo, from) >= kBarAxisSpan * (to - from)) {
                if (!barScale(u, vertical, nums, budget, limitHit)) {
                    return;
                }
                found.push_back({ u, vertical });
                if (found.size() >= kMaxBarRuns) {
                    full = true;
                    *limitHit = true;
                }
                return;
            }
        }
    };
    for (const bool vertical : { true, false }) {
        auto base = [&](const BarBox &b) { return vertical ? b.y1 : b.x0; };
        auto along = [&](const BarBox &b) { return vertical ? b.x0 : b.y0; };
        std::vector<size_t> order(filled.size());
        for (size_t i = 0; i < order.size(); ++i) {
            order[i] = i;
        }
        std::sort(order.begin(), order.end(), [&](size_t a, size_t b) { return base(filled[a]) < base(filled[b]); });
        size_t start = 0;
        while (start < order.size() && !full && *budget > 0) {
            // a baseline: bases within kBarBaseTolerance of its first
            const double first = base(filled[order[start]]);
            size_t end = start + 1;
            while (end < order.size() && base(filled[order[end]]) - first <= kBarBaseTolerance) {
                ++end;
            }
            std::sort(order.begin() + start, order.begin() + end, [&](size_t a, size_t b) { return along(filled[a]) < along(filled[b]); });
            std::vector<BarBox> run;
            double thick = 0;
            for (size_t k = start; k < end; ++k) {
                const BarBox &b = filled[order[k]];
                const double t = vertical ? b.x1 - b.x0 : b.y1 - b.y0;
                if (!run.empty()) {
                    const double gap = vertical ? b.x0 - run.back().x1 : b.y0 - run.back().y1;
                    if (gap < -0.5 || gap > kMaxBarGap * std::max(thick, t)) {
                        consider(run, vertical);
                        run.clear();
                        thick = 0;
                    }
                }
                run.push_back(b);
                thick = std::max(thick, t);
            }
            consider(run, vertical);
            start = end;
        }
    }
    // a run takes in the boxes stacked on it, and runs that now overlap are one cluster
    std::vector<BarBox> merged;
    for (auto [r, vertical] : found) {
        for (int round = 0; round < kBarStackRounds; ++round) {
            if (!spend(budget, filled.size() + 1, limitHit)) {
                break;
            }
            const BarBox before = r;
            for (const BarBox &b : filled) {
                const bool stacked = vertical ? (b.x0 >= r.x0 - 1 && b.x1 <= r.x1 + 1 && b.y0 <= r.y1 + 1 && b.y1 >= r.y0 - 1)
                                              : (b.y0 >= r.y0 - 1 && b.y1 <= r.y1 + 1 && b.x0 <= r.x1 + 1 && b.x1 >= r.x0 - 1);
                if (stacked) {
                    r = barUnion(r, b);
                }
            }
            if (r.x0 == before.x0 && r.y0 == before.y0 && r.x1 == before.x1 && r.y1 == before.y1) {
                break;
            }
        }
        if (!spend(budget, merged.size() + 1, limitHit)) {
            break;
        }
        bool joined = false;
        for (BarBox &o : merged) {
            if (std::min(o.x1, r.x1) > std::max(o.x0, r.x0) && std::min(o.y1, r.y1) > std::max(o.y0, r.y0)) {
                o = barUnion(o, r);
                joined = true;
                break;
            }
        }
        if (!joined) {
            merged.push_back(r);
        }
    }
    for (BarBox r : merged) {
        if (!spend(budget, filled.size() + strokes.size() + drawings.size() + 1, limitHit)) {
            break;
        }
        TextGraphic g {};
        // painted paths: the boxes inside the cluster's box (1 point of slack) ...
        for (const BarBox &b : filled) {
            if (b.x0 >= r.x0 - 1 && b.x1 <= r.x1 + 1 && b.y0 >= r.y0 - 1 && b.y1 <= r.y1 + 1) {
                ++g.paths;
            }
        }
        // ... and the stroked rules near it (axes, ticks, gridlines: within kBarRuleReach points, at most twice the cluster's
        // size, so a frame or a column rule is not taken), which also grow the cluster's box, as doppler-md grows a figure
        const BarBox core = r;
        const double reach = std::max(core.x1 - core.x0, core.y1 - core.y0);
        for (const BarBox &st : strokes) {
            const bool near = st.x1 >= core.x0 - kBarRuleReach && st.x0 <= core.x1 + kBarRuleReach && st.y1 >= core.y0 - kBarRuleReach && st.y0 <= core.y1 + kBarRuleReach;
            if (near && std::max(st.x1 - st.x0, st.y1 - st.y0) <= 2 * reach) {
                ++g.paths;
                r = barUnion(r, st);
            }
        }
        // a bar chart with a drawn trend line or curve is one figure, as in doppler-md: joined with every drawing cluster
        // it overlaps, repeated until no other overlaps the grown box (council r2 N3/L3), so clusters never overlap
        size_t into = drawings.size();
        bool grew = true;
        while (grew) {
            grew = false;
            for (size_t k = 0; k < drawings.size(); ++k) {
                if (!spend(budget, 1, limitHit)) {
                    break;
                }
                TextGraphic &d = drawings[k];
                if (k == into || !(std::min(d.xMax, r.x1) > std::max(d.xMin, r.x0) && std::min(d.yMax, r.y1) > std::max(d.yMin, r.y0))) {
                    continue;
                }
                r = barUnion(r, { d.xMin, d.yMin, d.xMax, d.yMax });
                if (into == drawings.size()) {
                    into = k;
                    d.paths += g.paths;
                } else {
                    TextGraphic &t = drawings[into];
                    t.paths += d.paths;
                    t.segments += d.segments;
                    t.curves += d.curves;
                    t.directions |= d.directions;
                    drawings.erase(drawings.begin() + std::ptrdiff_t(k));
                    if (k < into) {
                        --into;
                    }
                    --k;
                }
                grew = true;
            }
            if (*budget == 0) {
                break;
            }
        }
        if (into < drawings.size()) {
            TextGraphic &t = drawings[into];
            t.xMin = r.x0;
            t.yMin = r.y0;
            t.xMax = r.x1;
            t.yMax = r.y1;
            continue;
        }
        g.xMin = r.x0;
        g.yMin = r.y0;
        g.xMax = r.x1;
        g.yMax = r.y1;
        out.push_back(g);
    }
    return out;
}

} // namespace

//------------------------------------------------------------------------
// PageFacts
//------------------------------------------------------------------------

struct PageFacts::State
{
    int pageNum = 0;
    bool begun = false;
    bool ended = false;
    unsigned warnings = 0;
    unsigned reconstructionsAtBegin = 0;

    // geometry
    Page *page = nullptr;
    double pageW = 0, pageH = 0;

    // text
    uint64_t glyphs = 0, unmapped = 0, chars = 0, invisibleChars = 0, clippedChars = 0;
    uint64_t control = 0, privateUse = 0, replacement = 0, math = 0, vertical = 0, rotated = 0;
    double smallestFont = INFINITY;
    CoverageGrid textGrid;
    std::vector<uint64_t> charCells, invisibleCells; // per grid cell (oddtext)
    double cellsPerX = 0, cellsPerY = 0; // grid cells per point (0 for an empty page)
    std::vector<FontSeen> fonts;
    bool fontsTruncated = false;
    const TextFontInfo *lastInfo = nullptr;
    const FontSeen *lastFont = nullptr;
    // word boxes and whether each is visible (images-detail: words inside each listed image)
    struct WordBox
    {
        double x0, y0, x1, y1;
        bool visible;
    };
    std::vector<WordBox> wordBoxes;
    // word centres (vectors: bar charts hold few words; at most kMaxBarWords a page) and the page's numbers (bar scales)
    std::vector<WordCentre> wordCentres;
    std::vector<ScaleNumber> scaleNumbers;
    bool barWordsTruncated = false;
    // characters by word size in half points (vectors: the body size the grid search scales
    // with), the sizes in first-seen order, and whether a size fell outside the bins
    std::vector<uint64_t> sizeChars; // maxSizeBins, allocated once
    std::vector<int> sizeOrder;
    bool sizeOutOfRange = false;

    // vectors
    uint64_t paths = 0, filled = 0, stroked = 0;
    bool pathLimit = false;
    CoverageGrid vectorGrid;

    // annotations
    std::vector<AnnotDraw> draws;
    Annot *openAnnot = nullptr;
    uint64_t openMarks = 0, openGlyphs = 0;

    // the page line without its warnings, features and timing
    std::string body;
    unsigned features = 0;

    // kept across pages: indirect objects named by widgets (FormObjects), for the xref
    // generation they were fetched under
    FormCache formObjects;
    unsigned formObjectsGeneration = 0;

    void reset(int num)
    {
        // keep the buffers' memory: they are refilled for every page
        std::vector<uint64_t> cells = std::move(charCells), inv = std::move(invisibleCells);
        std::vector<FontSeen> fontBuffer = std::move(fonts);
        std::vector<AnnotDraw> drawBuffer = std::move(draws);
        std::vector<WordBox> wordBuffer = std::move(wordBoxes);
        std::vector<uint64_t> charsBuffer = std::move(sizeChars);
        std::vector<int> orderBuffer = std::move(sizeOrder);
        for (int bin : orderBuffer) {
            charsBuffer[bin] = 0; // only the bins this page touched
        }
        FormCache formCache = std::move(formObjects);
        const unsigned formGeneration = formObjectsGeneration;
        *this = State();
        formObjects = std::move(formCache);
        formObjectsGeneration = formGeneration;
        pageNum = num;
        cells.clear();
        inv.clear();
        fontBuffer.clear();
        drawBuffer.clear();
        wordBuffer.clear();
        orderBuffer.clear();
        wordBoxes = std::move(wordBuffer);
        sizeChars = std::move(charsBuffer);
        sizeOrder = std::move(orderBuffer);
        charCells = std::move(cells);
        invisibleCells = std::move(inv);
        fonts = std::move(fontBuffer);
        draws = std::move(drawBuffer);
    }
};

PageFacts::PageFacts(unsigned families) : fams(families & AllFamilies), needTally((families & (Vectors | Annots | Forms | Ocg)) != 0), st(new State) { }

PageFacts::~PageFacts()
{
    if (activeFacts == this) {
        activeFacts = nullptr;
    }
    delete st;
}

bool PageFacts::parseLevel(const char *text, unsigned *families)
{
    if (!strcmp(text, "basic")) {
        *families = 0;
        return true;
    }
    if (!strcmp(text, "full")) {
        *families = AllFamilies;
        return true;
    }
    return false;
}

bool PageFacts::parseInclude(const char *text, unsigned *families)
{
    static const struct
    {
        const char *name;
        unsigned family;
    } names[] = { { "fonts", Fonts }, { "oddtext", OddText }, { "images-detail", ImagesDetail }, { "vectors", Vectors }, { "forms", Forms }, { "annots", Annots }, { "ocg", Ocg }, { "structure", Structure } };
    unsigned out = 0;
    std::string_view rest(text);
    while (!rest.empty()) {
        const size_t comma = rest.find(',');
        const std::string_view item = rest.substr(0, comma);
        rest = comma == std::string_view::npos ? std::string_view() : rest.substr(comma + 1);
        if (item.empty()) {
            continue;
        }
        bool found = false;
        for (const auto &n : names) {
            if (item == n.name) {
                out |= n.family;
                found = true;
            }
        }
        if (!found) {
            return false;
        }
    }
    *families |= out;
    return true;
}

namespace {
void writeHeader(Json &j, const char *type)
{
    j.open();
    j.kenum("schema", PageFacts::schemaName);
    j.kenum("type", type);
}

void writeLevel(Json &j, unsigned fams)
{
    j.kenum("level", fams == PageFacts::AllFamilies ? "full" : "basic");
    if (fams != PageFacts::AllFamilies && fams != 0) {
        static const char *const names[] = { "fonts", "oddtext", "images-detail", "vectors", "forms", "annots", "ocg", "structure" };
        j.kopen("include", '[');
        for (int i = 0; i < 8; ++i) {
            if (fams & (1u << i)) {
                j.enumItem(names[i]);
            }
        }
        j.close(']');
    }
}

// Only when they are not the defaults, so a default run's lines do not change.
void writeScanThresholds(Json &j, const ScanThresholds &t)
{
    if (t.isDefault()) {
        return;
    }
    j.kopen("scanThresholds");
    j.kexact("minCoverage", t.minCoverage);
    j.kint("maxVisibleWords", static_cast<uint64_t>(t.maxVisibleWords));
    j.kexact("layerOverlap", t.layerOverlap);
    j.close();
}
} // namespace

std::string PageFacts::failedDocLine(int errorCode) const
{
    Json j;
    writeHeader(j, "doc");
    writeLevel(j, fams);
    writeScanThresholds(j, scanThresholds);
    if (errorCode == errEncrypted) {
        j.kbool("locked", true);
        j.kenum("encryption", "user-password-required");
        j.kmask("unusualFeatures", 1u << fEncryptedUser, featureNames, fXfaPresent + 1);
    } else {
        j.kenum("error", errorCode == errOpenFile ? "open-failed" : "damaged");
    }
    j.close();
    j.s += '\n';
    return j.s;
}

std::string PageFacts::docLine(PDFDoc *doc, bool passwordGiven)
{
    Json j;
    unsigned features = 0;
    writeHeader(j, "doc");
    writeLevel(j, fams);
    writeScanThresholds(j, scanThresholds);
    XRef *xref = doc->getXRef();
    Catalog *catalog = doc->getCatalog();
    j.kint("pageCount", static_cast<uint64_t>(std::max(doc->getNumPages(), 0)));
    const int major = doc->getPDFMajorVersion(), minor = doc->getPDFMinorVersion();
    if (major >= 1 && major <= 2 && minor >= 0 && minor <= 9) {
        const char version[4] = { static_cast<char>('0' + major), '.', static_cast<char>('0' + minor), 0 };
        j.kenum("pdfVersion", version);
    }
    j.kbool("linearized", doc->isLinearized());
    // The doc line's reads (catalog, /Info, /Encrypt) are made under a RepairGuard, so they
    // cannot rebuild or extend the table the pages are drawn from: a run with facts and one
    // without draw the same objects. A repair the guard refused (a broken or missing object
    // the doc line reads nothing from) turns "ok" into "repaired" at the end.
    const int reconstructionsAtXref = xref->getReconstructionCount();
    openReconstructions = reconstructionsAtXref;
    const XRef::RepairGuard noRepair(xref);
    const unsigned int suppressedAtXref = xref->getSuppressedRepairs();
    auto trailerLookup = [&](const char *key) { return xref->getTrailerDict()->dictLookup(key); };
    const size_t xrefAt = j.s.size();
    j.kenum("xref", reconstructionsAtXref > 0 ? "repaired" : "ok");
    if (xref->isEncrypted()) {
        bool emptyWorks = !passwordGiven;
        if (passwordGiven) {
            Object encrypt = trailerLookup("Encrypt");
            if (encrypt.isDict()) {
                SecurityHandler *handler = SecurityHandler::make(doc, &encrypt);
                if (handler) {
                    emptyWorks = handler->isUnencrypted() || handler->checkEncryption(std::nullopt, std::nullopt);
                    delete handler;
                }
            }
        }
        j.kenum("encryption", emptyWorks ? "empty-user-password" : "user-password-required");
        features |= 1u << (emptyWorks ? fEncryptedEmpty : fEncryptedUser);
        j.kopen("permissions");
        j.kbool("print", xref->okToPrint(true));
        j.kbool("printHighRes", xref->okToPrintHighRes(true));
        j.kbool("modify", xref->okToChange(true));
        j.kbool("copy", xref->okToCopy(true));
        j.kbool("annotate", xref->okToAddNotes(true));
        j.kbool("fillForms", xref->okToFillForm(true));
        j.kbool("accessibility", xref->okToAccessibility(true));
        j.kbool("assemble", xref->okToAssemble(true));
        j.close();
    } else {
        j.kenum("encryption", "none");
    }
    j.kbool("locked", false);

    // XRef::getCatalog would rebuild the table when the catalog does not parse
    Object catDict = xref->fetch(xref->getRootNum(), xref->getRootGen());
    auto catLookup = [&](const char *key) { return catDict.isDict() ? catDict.dictLookup(key) : Object::null(); };
    if (wants(Structure)) {
        const int sections = xref->countSections(1000, 30000000LL);
        if (sections > 0) {
            const int linearizedSections = doc->isLinearized() ? 1 : 0;
            j.kint("incrementalUpdates", static_cast<uint64_t>(std::max(0, sections - 1 - linearizedSections)));
        }
        j.kbool("tagged", catDict.isDict() && !catDict.dictLookupNF("StructTreeRoot").isNull());
        Object markInfo = catLookup("MarkInfo");
        Object marked = markInfo.isDict() ? markInfo.dictLookup("Marked") : Object::null();
        j.kbool("markedContent", marked.isBool() && marked.getBool());
        if (const std::optional<std::string> lang = langTag(catLookup("Lang"))) {
            j.kenum("lang", lang->c_str());
        }
        bool outlineTruncated = false;
        j.kint("outlineCount", static_cast<uint64_t>(countOutline(catLookup("Outlines"), &outlineTruncated)));
        j.kbool("pageLabels", !catLookup("PageLabels").isNull());
        Object names = catLookup("Names");
        bool filesTruncated = false;
        const uint64_t files = names.isDict() ? countNameTree(names.dictLookup("EmbeddedFiles"), &filesTruncated) : 0;
        j.kint("embeddedFiles", files);
        if (files > 0) {
            features |= 1u << fEmbeddedFiles;
        }
        Object openAction = catLookup("OpenAction");
        Object aa = catLookup("AA");
        bool js = names.isDict() && !names.dictLookupNF("JavaScript").isNull();
        bool launch = false;
        const std::string openKind = actionKind(openAction);
        js = js || openKind == "JavaScript";
        launch = openKind == "Launch";
        if (aa.isDict()) {
            for (int i = 0; i < std::min(aa.dictGetLength(), 32); ++i) {
                Object action = aa.dictGetVal(i);
                const std::string kind = actionKind(action);
                js = js || kind == "JavaScript";
                launch = launch || kind == "Launch";
            }
        }
        j.kbool("javascript", js);
        j.kbool("openAction", !openAction.isNull());
        j.kbool("launchAction", launch);
        if (js) {
            features |= 1u << fJavascript;
        }
        bool stringsTruncated = false;
        Object info = trailerLookup("Info");
        if (info.isDict()) {
            Object producer = info.dictLookup("Producer");
            if (producer.isString()) {
                j.kstring("producer", sanitize(producer.getString(), &stringsTruncated));
            }
            Object creator = info.dictLookup("Creator");
            if (creator.isString()) {
                j.kstring("creator", sanitize(creator.getString(), &stringsTruncated));
            }
        }
        if (outlineTruncated || filesTruncated || stringsTruncated) {
            j.kopen("truncated", '[');
            if (filesTruncated) {
                j.enumItem("embeddedFiles");
            }
            if (outlineTruncated) {
                j.enumItem("outlineCount");
            }
            if (stringsTruncated) {
                j.enumItem("strings");
            }
            j.close(']');
        }
    }
    if (wants(Forms)) {
        Object acroForm = catLookup("AcroForm");
        j.kenum("acroForm", acroForm.isDict() ? "present" : "none");
        Object needAppearances = acroForm.isDict() ? acroForm.dictLookup("NeedAppearances") : Object::null();
        j.kbool("needAppearances", needAppearances.isBool() && needAppearances.getBool());
        const char *xfa = acroForm.isDict() ? xfaKind(acroForm.dictLookup("XFA")) : "none";
        j.kenum("xfa", xfa);
        if (strcmp(xfa, "none") != 0) {
            features |= 1u << fXfaPresent;
        }
    }
    if (wants(Ocg)) {
        const OCGs *ocgs = catalog ? catalog->getOptContentConfig() : nullptr;
        uint64_t count = 0, off = 0;
        if (ocgs && ocgs->isOk()) {
            for (const auto &entry : ocgs->getOCGs()) {
                ++count;
                off += entry.second->getState() == OptionalContentGroup::Off ? 1 : 0;
            }
        }
        j.kint("ocgCount", count);
        j.kint("ocgOffByDefault", off);
    }
    j.kmask("unusualFeatures", features, featureNames, fXfaPresent + 1);
    j.close();
    if (reconstructionsAtXref == 0 && xref->getSuppressedRepairs() != suppressedAtXref) {
        const size_t ok = j.s.find("\"ok\"", xrefAt);
        if (ok != std::string::npos) {
            j.s.replace(ok, 4, "\"repaired\"");
        }
    }
    j.s += '\n';
    docXref = xref;
    // rebuilds while the document was opened happen for every page range (the doc line's reads
    // are guarded and cannot add one)
    docReconstructions = xref->getReconstructionCount();
    return j.s;
}

void PageFacts::beginPage(int pageNum)
{
    st->reset(pageNum);
    st->begun = true;
    st->reconstructionsAtBegin = docXref ? docXref->getReconstructionCount() : 0;
    tally.reset();
    activeFacts = this;
}

void PageFacts::noteError(ErrorCategory category, const char *format)
{
    PageFacts *f = activeFacts;
    if (!f || !f->st->begun || !format) {
        return;
    }
    unsigned &w = f->st->warnings;
    switch (category) {
    case errSyntaxWarning:
        w |= 1u << wSyntaxWarning;
        break;
    case errSyntaxError:
        w |= 1u << wSyntaxError;
        break;
    case errIO:
        w |= 1u << wIO;
        break;
    case errUnimplemented:
        w |= 1u << wUnimplemented;
        break;
    case errInternal:
        w |= 1u << wInternal;
        break;
    default:
        break;
    }
    // the stage, from a few fixed prefixes of the format string (never formatted: its arguments,
    // which can quote the document, are not read); a group whose warning the page already has is
    // skipped, so an error-dense page costs a few comparisons per error
    static const char *const contentPrefixes[] = {
        "Unknown operator", "Too few (",     "Too many (", "Too many args", "Leftover args", "Arg #", "Weird page contents", "Illegal character", "Unterminated", "End of file inside", "Found state under last state guard",
        "No current point", "Mismatched EMC"
    };
    static const char *const fontPrefixes[] = { "No font in show", "Unknown font tag", "Unknown font type", "Couldn't find a font", "Couldn't create a font", "Bad font type" };
    static const char *const imagePrefixes[] = { "Bad image parameters", "Bad image", "Unknown image", "JPX", "JBIG2", "Bad DCT", "DCT", "CCITT", "Invalid Code", "Bad 2D code", "Bad white code", "Bad black code" };
    // fixed messages of poppler's own budgets (optional content expressions and the page's operand budget,
    // image colour spaces, marked-content nesting, filter chains)
    static const char *const limitPrefixes[] = { "Optional content visibility expression too large", "Optional content visibility budget", "Image colour space budget exhausted", "Marked content nested too deeply",
                                                 "Too many stream filters" };
    auto note = [&w, format](Warning warning, const auto &prefixes) {
        if (w & (1u << warning)) {
            return;
        }
        for (const char *p : prefixes) {
            if (startsWith(format, p)) {
                w |= 1u << warning;
                return;
            }
        }
    };
    note(wBadContent, contentPrefixes);
    note(wMissingFont, fontPrefixes);
    note(wBrokenImage, imagePrefixes);
    note(wLimit, limitPrefixes);
}

void PageFacts::startPage(Page *page, double pageWidth, double pageHeight)
{
    if (!st->begun) {
        // a device driven without beginPage (not by a facts driver): collect anyway
        st->reset(page ? page->getNum() : 0);
        st->begun = true;
        tally.reset();
    }
    st->page = page;
    st->pageW = pageWidth;
    st->pageH = pageHeight;
    if (wants(OddText)) {
        st->charCells.assign(CoverageGrid::size * CoverageGrid::size, 0);
        st->invisibleCells.assign(CoverageGrid::size * CoverageGrid::size, 0);
    }
    const bool usable = pageWidth > 0 && pageHeight > 0 && std::isfinite(pageWidth) && std::isfinite(pageHeight);
    st->cellsPerX = usable ? CoverageGrid::size / pageWidth : 0;
    st->cellsPerY = usable ? CoverageGrid::size / pageHeight : 0;
}

const PageFacts::FontSeen *PageFacts::fontOf(const TextFontInfo *info)
{
    if (info == st->lastInfo) {
        return st->lastFont;
    }
    const GfxFont *font = info ? info->getGfxFont() : nullptr;
    st->lastInfo = info;
    st->lastFont = nullptr;
    if (!font) {
        return nullptr;
    }
    for (const FontSeen &f : st->fonts) {
        if (f.font == font) {
            st->lastFont = &f;
            return &f;
        }
    }
    if (st->fonts.capacity() < maxFonts) {
        st->fonts.reserve(maxFonts); // FontSeen pointers stay valid while the page lasts
    }
    if (st->fonts.size() >= maxFonts) {
        st->fontsTruncated = true;
        return nullptr;
    }
    FontSeen f {};
    f.font = font;
    f.type3 = font->getType() == fontType3;
    Ref embedded;
    f.embedded = f.type3 || font->getEmbeddedFontID(&embedded);
    f.cid = font->isCIDFont();
    f.toUnicode = font->hasToUnicodeCMap();
    f.symbolic = font->isSymbolic();
    f.vertical = font->getWMode() == GfxFont::WritingMode::Vertical;
    const std::optional<std::string> &name = font->getName();
    if (name) {
        const std::string &n = *name;
        f.ocr = n.find("GlyphLessFont") != std::string::npos || n.find("GlyphLess") != std::string::npos;
        static const char *const mathNames[] = { "CMMI", "CMSY", "CMEX", "CMBSY", "MSAM", "MSBM", "Symbol", "Math", "STIX", "Euler", "EUFM", "EUSM", "rsfs", "esint", "txsy", "txex", "pxsy", "NewCM-Math" };
        for (const char *m : mathNames) {
            f.math = f.math || n.find(m) != std::string::npos;
        }
    }
    st->fonts.push_back(f);
    st->lastFont = &st->fonts.back();
    return st->lastFont;
}

void PageFacts::onUnmappedGlyph(const GfxState * /*state*/, const TextFontInfo *font, double fontSize)
{
    add(st->glyphs, 1);
    add(st->unmapped, 1);
    if (wants(Fonts)) {
        fontOf(font);
        if (fontSize > 0 && std::isfinite(fontSize)) {
            st->smallestFont = std::min(st->smallestFont, fontSize);
        }
    }
}

GlyphVisibility glyphVisibility(const GfxState *state, double x, double y, double dx, double dy, double fontSize, int rot)
{
    // the glyph's centre: half its advance along the baseline, a third of the size up
    double cx = x + 0.5 * dx, cy = y + 0.5 * dy;
    const double up = 0.3 * fontSize;
    switch (rot) {
    case 0:
        cy -= up;
        break;
    case 1:
        cx += up;
        break;
    case 2:
        cy += up;
        break;
    default:
        cx -= up;
        break;
    }
    double clipX0, clipY0, clipX1, clipY1;
    state->getClipBBox(&clipX0, &clipY0, &clipX1, &clipY1);
    GlyphVisibility v;
    v.invisible = state->getRender() == 3;
    v.clipped = !(cx >= clipX0 && cx <= clipX1 && cy >= clipY0 && cy <= clipY1);
    v.cx = cx;
    v.cy = cy;
    return v;
}

void PageFacts::onGlyph(const TextFontInfo *font, double fontSize, const Unicode *u, int uLen, int rot, bool diagonal, GlyphVisibility visibility)
{
    // hot: once per glyph drawn. Counts are bounded by the content stream's size, far below
    // 2^64, so they are plain increments here.
    State &s = *st;
    const auto n = static_cast<uint64_t>(uLen);
    ++s.glyphs;
    s.chars += n;
    const bool invisible = visibility.invisible, clipped = visibility.clipped;
    const double cx = visibility.cx, cy = visibility.cy;
    s.clippedChars += clipped ? n : 0;
    s.invisibleChars += (invisible || clipped) ? n : 0;
    if (!(fams & (Fonts | OddText))) {
        return;
    }
    const FontSeen *f = fontOf(font);
    if (fontSize > 0 && fontSize < s.smallestFont) {
        s.smallestFont = fontSize;
    }
    if (!(fams & OddText)) {
        return;
    }
    const bool vertical = f && f->vertical;
    // vertical writing turns lines a quarter (TextPage::beginWord): that is their upright
    const bool turned = diagonal || rot != (vertical ? 1 : 0);
    if (f && f->math) {
        s.math += n;
    }
    for (int i = 0; i < uLen; ++i) {
        const Unicode c = u[i];
        if (c >= 0x20 && c < 0x7f) {
            continue; // printable ASCII: none of the classes below
        }
        if (c < 0x20 || (c >= 0x7f && c <= 0x9f)) {
            ++s.control;
        } else if ((c >= 0xe000 && c <= 0xf8ff) || (c >= 0xf0000 && c <= 0x10fffd)) {
            ++s.privateUse;
        } else if (c == 0xfffd) {
            ++s.replacement;
        }
        if (!(f && f->math) && ((c >= 0x2200 && c <= 0x22ff) || (c >= 0x2a00 && c <= 0x2aff) || (c >= 0x27c0 && c <= 0x27ef) || (c >= 0x2980 && c <= 0x29ff) || (c >= 0x1d400 && c <= 0x1d7ff))) {
            ++s.math;
        }
    }
    s.vertical += vertical ? n : 0;
    s.rotated += turned ? n : 0;
    // truncation is floor for the non-negative values kept (NaN fails the tests)
    const double col = cx * s.cellsPerX, row = cy * s.cellsPerY;
    if (col >= 0 && col < CoverageGrid::size && row >= 0 && row < CoverageGrid::size) {
        const int cell = static_cast<int>(row) * CoverageGrid::size + static_cast<int>(col);
        s.charCells[cell] += n;
        s.invisibleCells[cell] += (invisible || clipped) ? n : 0;
    }
}

void PageFacts::onWord(const TextWord *word)
{
    double x0, y0, x1, y1;
    word->getBBox(&x0, &y0, &x1, &y1);
    State &s = *st;
    s.textGrid.mark(x0, y0, x1, y1, s.pageW, s.pageH);
    if (wants(ImagesDetail)) {
        // visible as TextPage counts it (getInvisibleWordCount)
        s.wordBoxes.push_back({ x0, y0, x1, y1, word->isShown() && !word->isInvisible() });
    }
    if (wants(Vectors) && std::isfinite(x0 + x1) && std::isfinite(y0 + y1)) {
        if (s.wordCentres.size() < kMaxBarWords) {
            s.wordCentres.push_back({ (x0 + x1) / 2, (y0 + y1) / 2 });
        } else {
            s.barWordsTruncated = true;
        }
        double v;
        const int n = word->getLength();
        if (s.scaleNumbers.size() < kMaxScaleNumbers && n > 0 && n <= 16) {
            Unicode text[16];
            for (int i = 0; i < n; ++i) {
                text[i] = *word->getChar(i);
            }
            if (scaleNumber(text, n, &v)) {
                s.scaleNumbers.push_back({ x0, y0, x1, y1, v });
            }
        }
    }
    if (wants(Vectors)) {
        // pdftomarkdown's body size: the most common size rounded to half a point, by characters
        // (a size outside the bins makes the body size uncertain: gridsTruncated)
        const double size = word->getFontSize();
        if (size != 0) {
            const double half = std::nearbyint(size * 2.0); // NaN and infinities fail the range test
            if (half >= 0 && half < maxSizeBins) {
                if (s.sizeChars.empty()) {
                    s.sizeChars.assign(maxSizeBins, 0);
                }
                const int bin = static_cast<int>(half);
                if (s.sizeChars[bin] == 0) {
                    s.sizeOrder.push_back(bin);
                }
                // never 0 once touched, so the first-seen order is kept
                s.sizeChars[bin] += static_cast<uint64_t>(std::max(word->getLength(), 1));
            } else {
                s.sizeOutOfRange = true;
            }
        }
    }
}

void PageFacts::onPath(const GfxState *state, bool filled)
{
    State &s = *st;
    if (s.paths >= maxPaths) {
        s.pathLimit = true;
        return;
    }
    ++s.paths;
    ++(filled ? s.filled : s.stroked);
    const GfxPath *path = state->getPath();
    double x0 = INFINITY, y0 = INFINITY, x1 = -INFINITY, y1 = -INFINITY;
    for (int i = 0; i < path->getNumSubpaths(); ++i) {
        const GfxSubpath *sub = path->getSubpath(i);
        for (int k = 0; k < sub->getNumPoints(); ++k) {
            double x, y;
            state->transform(sub->getX(k), sub->getY(k), &x, &y);
            if (!std::isfinite(x) || !std::isfinite(y)) {
                return;
            }
            x0 = std::min(x0, x);
            y0 = std::min(y0, y);
            x1 = std::max(x1, x);
            y1 = std::max(y1, y);
        }
    }
    double cx0, cy0, cx1, cy1;
    state->getClipBBox(&cx0, &cy0, &cx1, &cy1);
    s.vectorGrid.mark(std::max(x0, cx0), std::max(y0, cy0), std::min(x1, cx1), std::min(y1, cy1), s.pageW, s.pageH);
}

uint64_t PageFacts::paintMarks() const
{
    // painting operators executed while shown, plus glyphs added to the page
    static const std::vector<int> paintOps = [] {
        static const char *const names[] = { "S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "sh", "Do", "BI", "Tj", "TJ", "'", "\"" };
        std::vector<int> ops;
        for (int i = 0; i < Gfx::opCount(); ++i) {
            for (const char *n : names) {
                if (!strcmp(Gfx::opName(i), n)) {
                    ops.push_back(i);
                }
            }
        }
        return ops;
    }();
    uint64_t n = st->glyphs;
    for (int i : paintOps) {
        add(n, tally.visible[i]);
        add(n, tally.inLayer[i]);
    }
    return n;
}

void PageFacts::beginAnnot(Annot *annot)
{
    st->openAnnot = annot;
    st->openMarks = paintMarks();
    st->openGlyphs = st->glyphs;
}

void PageFacts::endAnnot(Annot *annot)
{
    if (st->openAnnot != annot || st->draws.size() >= maxAnnots) {
        st->openAnnot = nullptr;
        return;
    }
    st->draws.push_back({ annot, paintMarks() - st->openMarks, st->glyphs - st->openGlyphs });
    st->openAnnot = nullptr;
}

namespace {

// form field facts from a widget's dictionary and its /Parent chain
struct WidgetField
{
    enum Kind
    {
        Unknown,
        Text,
        Checkbox,
        Radio,
        PushButton,
        Choice,
        Signature
    } kind = Unknown;
    bool filled = false; // the value says so
    bool appearanceOn = false; // a button's /AS is not Off
};

// What widgetField tests of a value, without its payload: a string becomes "" (empty), the bare
// UTF-16 byte order mark, or "x" (anything else). Object::copy copies a string's bytes, so a
// string /V that thousands of widgets share would otherwise be copied for every one of them;
// dicts, arrays and streams are shared, and copy in constant time.
Object formSummary(const Object &obj)
{
    if (!obj.isString()) {
        return obj.copy();
    }
    const std::string &str = obj.getString();
    return Object(std::string(str.empty() ? "" : str == "\xfe\xff" ? "\xfe\xff" : "x"));
}

// Indirect objects that widgets name (/Parent, /V, /FT, /Ff, /AP), each fetched once per
// document: thousands of widgets (on every page sharing one /Annots array) can name one field or
// one large /V, and XRef::fetch parses an object again on every call. Only summaries are kept
// (formSummary). The work is bounded, not just the cache: once maxFormObjects distinct objects
// (or maxFormBytes of strings) have been fetched, nothing more is fetched for the document and
// the forms read as truncated. Nothing is evicted and fetched again: that would bring back the
// re-parse per widget. Compressed objects are keyed without their generation, which
// XRef::fetch ignores for them ("4 0 R" and "4 7 R" are the same object, and one cache entry).
class FormObjects
{
public:
    FormObjects(FormCache &cacheA, XRef *xrefA) : cache(cacheA), xref(xrefA) { }
    Object get(const Object &dict, const char *key)
    {
        if (!dict.isDict()) {
            return Object::null();
        }
        const Object &nf = dict.dictLookupNF(key);
        if (!nf.isRef()) {
            return formSummary(nf);
        }
        if (!xref) {
            cache.exhausted = true; // no table to fetch through once: do not parse per widget
            return Object::null();
        }
        Ref ref = nf.getRef();
        if (ref.num < 0 || ref.num >= xref->getNumObjects()) {
            return Object::null(); // XRef::fetch gives null without reading anything
        }
        if (xref->getEntry(ref.num, false)->type == xrefEntryCompressed) {
            ref.gen = 0;
        }
        const auto it = cache.objects.find(ref);
        if (it != cache.objects.end()) {
            return it->second.copy();
        }
        if (cache.exhausted || cache.objects.size() >= maxFormObjects || cache.stringBytes >= maxFormBytes) {
            cache.exhausted = true;
            return Object::null();
        }
        const Object obj = xref->fetch(nf.getRef());
        if (obj.isString()) {
            cache.stringBytes += obj.getString().size();
        }
        Object summary = formSummary(obj);
        cache.objects.emplace(ref, summary.copy());
        return summary;
    }
    bool exhausted() const { return cache.exhausted; }

private:
    FormCache &cache;
    XRef *xref;
};

WidgetField widgetField(const Object &widget, FormObjects &objects)
{
    WidgetField w;
    Object ft = Object::null(), ff = Object::null(), v = Object::null();
    Object node = widget.copy();
    std::set<Ref> seen;
    for (int depth = 0; depth < 32 && node.isDict(); ++depth) {
        if (ft.isNull()) {
            ft = objects.get(node, "FT");
        }
        if (ff.isNull()) {
            ff = objects.get(node, "Ff");
        }
        if (v.isNull()) {
            v = objects.get(node, "V");
        }
        const Object &parentRef = node.dictLookupNF("Parent");
        if (parentRef.isRef() && !seen.insert(parentRef.getRef()).second) {
            break;
        }
        node = objects.get(node, "Parent");
    }
    const int flags = ff.isInt() ? ff.getInt() : 0;
    if (ft.isName("Tx")) {
        w.kind = WidgetField::Text;
        w.filled = (v.isString() && !v.getString().empty() && v.getString() != "\xfe\xff") || (v.isStream());
    } else if (ft.isName("Ch")) {
        w.kind = WidgetField::Choice;
        w.filled = (v.isString() && !v.getString().empty() && v.getString() != "\xfe\xff") || (v.isArray() && v.arrayGetLength() > 0);
    } else if (ft.isName("Sig")) {
        w.kind = WidgetField::Signature;
        w.filled = v.isDict();
    } else if (ft.isName("Btn")) {
        w.kind = (flags & (1 << 16)) ? WidgetField::PushButton : (flags & (1 << 15)) ? WidgetField::Radio : WidgetField::Checkbox;
        // a widget is on when the field's value names its own on state (a key of /AP /N other
        // than Off: each radio button of a group has its own), or any state other than Off
        // when the widget has no appearances to name it
        std::string onState;
        Object ap = objects.get(widget, "AP");
        Object normal = ap.isDict() ? objects.get(ap, "N") : Object::null();
        if (normal.isDict()) {
            for (int i = 0; i < std::min(normal.dictGetLength(), 16) && onState.empty(); ++i) {
                const char *key = normal.getDict()->getKey(i).c_str();
                if (key && strcmp(key, "Off") != 0) {
                    onState = key;
                }
            }
        }
        w.filled = v.isName() && !v.isName("Off") && (onState.empty() || v.getNameString() == onState);
        Object as = objects.get(widget, "AS");
        w.appearanceOn = as.isName() && !as.isName("Off");
    }
    return w;
}

} // namespace

void PageFacts::endPage(const TextPage &text)
{
    // issue 45: the facts' own reads (annotations, widgets, forms) cannot rebuild the table
    std::optional<XRef::RepairGuard> noRepair;
    if (docXref) {
        noRepair.emplace(docXref);
    }
    State &s = *st;
    s.ended = true;
    Json j;
    j.warnings = &s.warnings;
    unsigned features = 0;
    const double pageW = text.getPageWidth(), pageH = text.getPageHeight();
    const double area = pageW * pageH;
    auto ratio = [](uint64_t n, uint64_t d) { return d == 0 ? 0.0 : double(n) / double(d); };

    // A: geometry
    if (Page *page = s.page) {
        const PDFRectangle &media = page->getMediaBox();
        const PDFRectangle &crop = page->getCropBox();
        int rotation = page->getRotate() % 360;
        rotation = ((rotation < 0 ? rotation + 360 : rotation) + 45) / 90 * 90 % 360;
        double userUnit = 1;
        const Object &pageObj = page->getPageObj();
        if (pageObj.isDict()) {
            Object uu = pageObj.dictLookup("UserUnit");
            if (uu.isNum() && std::isfinite(uu.getNum()) && uu.getNum() > 0) {
                userUnit = std::clamp(uu.getNum(), 0.0001, 1e6);
            }
        }
        double w = std::fabs(crop.x2 - crop.x1), h = std::fabs(crop.y2 - crop.y1);
        if (rotation == 90 || rotation == 270) {
            std::swap(w, h);
        }
        j.kopen("geometry");
        j.kpt("widthPt", w);
        j.kpt("heightPt", h);
        j.kint("rotation", static_cast<uint64_t>(rotation));
        j.kunit("userUnit", userUnit);
        if (pointsDiffer(media, crop)) {
            j.kbox("mediaBox", media.x1, media.y1, media.x2, media.y2);
            j.kbox("cropBox", crop.x1, crop.y1, crop.x2, crop.y2);
        }
        j.close();
        if (std::max(w, h) * userUnit > hugePagePt) {
            features |= 1u << fHugePage;
        }
        if (rotation != 0) {
            features |= 1u << fRotatedPage;
        }
    }

    // C (computed first: text over images needs the image grid)
    const std::vector<TextImage> &images = text.getImages();
    CoverageGrid imageGrid;
    for (const TextImage &im : images) {
        imageGrid.mark(im.xMin, im.yMin, im.xMax, im.yMax, pageW, pageH);
    }
    const ScanVerdict scan = classifyScan(images, pageW, pageH, text.getVisibleWordCount(), scanThresholds);

    // B: text
    const uint64_t words = text.getWordCount(), invisibleWords = text.getInvisibleWordCount();
    j.kopen("text");
    j.kint("charCount", s.chars);
    j.kint("visibleCharCount", s.chars - std::min(s.chars, s.invisibleChars));
    j.kint("invisibleCharCount", std::min(s.chars, s.invisibleChars));
    j.kint("wordCount", words);
    j.kint("visibleWordCount", words - invisibleWords);
    j.kint("invisibleWordCount", invisibleWords);
    j.kenum("textLayer", words == 0 ? "none" : invisibleWords == words ? "invisible" : invisibleWords == 0 ? "visible" : "mixed");
    j.kratio("textCoverage", s.textGrid.share());
    if (wants(OddText)) {
        j.kint("clippedCharCount", s.clippedChars);
        if (s.chars > 0) {
            j.kratio("controlCharRatio", ratio(s.control, s.chars));
            j.kratio("privateUseRatio", ratio(s.privateUse, s.chars));
            j.kratio("replacementCharRatio", ratio(s.replacement, s.chars));
            j.kratio("mathGlyphRatio", ratio(s.math, s.chars));
            j.kratio("verticalCharRatio", ratio(s.vertical, s.chars));
            j.kratio("rotatedTextRatio", ratio(s.rotated, s.chars));
            uint64_t overImage = 0, invisibleOverImage = 0, invisibleTotal = 0;
            for (int r = 0; r < CoverageGrid::size; ++r) {
                for (int c = 0; c < CoverageGrid::size; ++c) {
                    const uint64_t n = s.charCells[r * CoverageGrid::size + c];
                    const uint64_t inv = s.invisibleCells[r * CoverageGrid::size + c];
                    add(invisibleTotal, inv);
                    if ((imageGrid.rows[r] >> c) & 1) {
                        add(overImage, n);
                        add(invisibleOverImage, inv);
                    }
                }
            }
            j.kratio("textOverImageRatio", ratio(overImage, s.chars));
            j.kbool("invisibleOverFullPageImage", scan.fullPageImage && invisibleTotal > 0 && invisibleOverImage * 2 >= invisibleTotal);
            if (s.chars >= rotatedContentMinChars && ratio(s.rotated, s.chars) >= rotatedContentShare) {
                features |= 1u << fRotatedContent;
            }
        }
    }
    j.close();

    // B: fonts
    if (wants(Fonts)) {
        uint64_t type3 = 0, nonEmbedded = 0, cid = 0, noToUnicode = 0;
        bool ocr = false, guessed = false;
        for (const FontSeen &f : s.fonts) {
            type3 += f.type3;
            nonEmbedded += !f.embedded;
            cid += f.cid;
            noToUnicode += !f.toUnicode;
            ocr = ocr || f.ocr;
            guessed = guessed || (!f.toUnicode && (f.cid || f.type3 || f.symbolic));
        }
        j.kopen("fonts");
        j.kint("count", s.fonts.size());
        if (std::isfinite(s.smallestFont)) {
            j.kpt("smallestPt", s.smallestFont);
        }
        j.kint("type3", type3);
        j.kint("nonEmbedded", nonEmbedded);
        j.kint("cid", cid);
        j.kint("noToUnicode", noToUnicode);
        j.kint("glyphCount", s.glyphs);
        if (s.glyphs > 0) {
            j.kratio("unmappedGlyphRatio", ratio(s.unmapped, s.glyphs));
        }
        j.kbool("ocrFontHint", ocr);
        j.kbool("truncated", s.fontsTruncated);
        j.close();
        if (type3 > 0) {
            features |= 1u << fType3Font;
        }
        if (guessed) {
            features |= 1u << fNoToUnicode;
        }
        if (s.fontsTruncated) {
            s.warnings |= 1u << wLimit;
        }
    }

    // C: images
    j.kopen("images");
    j.kint("imageCount", images.size());
    j.kratio("imageCoverage", imageGrid.share());
    j.kratio("largestImageCoverage", scan.largest);
    j.kbool("fullPageImage", scan.fullPageImage);
    j.kenum("scan", scanKindName(scan.kind));
    if (scan.kind != ScanVerdict::None) {
        j.kint("scanImage", static_cast<uint64_t>(scan.image));
        features |= 1u << fScanPage;
        if (scan.kind == ScanVerdict::Layered) {
            features |= 1u << fLayeredScan;
        }
    }
    if (wants(ImagesDetail)) {
        uint64_t masks = 0, inlines = 0;
        for (const TextImage &im : images) {
            masks += im.stencil;
            inlines += im.info.inlineImg;
        }
        // words whose box lies inside each listed image's box (within wordInsideSlack points),
        // and words whose box centre does (issue 43), [visible, invisible]
        const size_t listed = std::min(images.size(), maxItems);
        std::array<std::array<uint64_t, 2>, maxItems> wordsInside {}, centredInside {};
        for (const State::WordBox &w : s.wordBoxes) {
            const double cx = 0.5 * w.x0 + 0.5 * w.x1, cy = 0.5 * w.y0 + 0.5 * w.y1; // NaN fails every test
            for (size_t i = 0; i < listed; ++i) {
                const TextImage &im = images[i];
                const double ix0 = im.xMin - wordInsideSlack, iy0 = im.yMin - wordInsideSlack, ix1 = im.xMax + wordInsideSlack, iy1 = im.yMax + wordInsideSlack;
                if (w.x0 >= ix0 && w.y0 >= iy0 && w.x1 <= ix1 && w.y1 <= iy1) {
                    ++wordsInside[i][w.visible ? 0 : 1];
                }
                if (cx >= ix0 && cy >= iy0 && cx <= ix1 && cy <= iy1) {
                    ++centredInside[i][w.visible ? 0 : 1];
                }
            }
        }
        j.kint("maskCount", masks);
        j.kint("inlineCount", inlines);
        j.kopen("items", '[');
        for (size_t i = 0; i < images.size() && i < maxItems; ++i) {
            const TextImage &im = images[i];
            j.open();
            j.kint("index", i + 1);
            j.kbox("bbox", im.xMin, im.yMin, im.xMax, im.yMax);
            j.kratio("coverage", area > 0 ? (im.xMax - im.xMin) * (im.yMax - im.yMin) / area : 0.0);
            j.kopen("px", '[');
            j.integer(static_cast<uint64_t>(std::max(im.pixelWidth, 0)));
            j.integer(static_cast<uint64_t>(std::max(im.pixelHeight, 0)));
            j.close(']');
            j.kopen("dpi", '[');
            j.point(im.dpiX);
            j.point(im.dpiY);
            j.close(']');
            j.kenum("filter", imageFilterName(im.info.filter));
            if (im.info.bpc > 0) {
                j.kint("bpc", im.info.bpc);
            }
            j.kenum("colorSpace", imageColorName(im.info.color));
            j.kbool("imageMask", im.stencil);
            j.kbool("softMask", im.info.softMask);
            j.kbool("mask", im.info.mask);
            j.kbool("decode", im.info.decode);
            j.kbool("inline", im.info.inlineImg);
            if (im.ref != Ref::INVALID() && im.ref.num > 0) {
                j.kint("obj", static_cast<uint64_t>(im.ref.num));
            }
            j.kenum("copy", imageCopyName(im.info.copy));
            j.kint("visibleWordsInside", wordsInside[i][0]);
            j.kint("invisibleWordsInside", wordsInside[i][1]);
            j.kint("visibleWordsCentredInside", centredInside[i][0]);
            j.kint("invisibleWordsCentredInside", centredInside[i][1]);
            // encoded bytes over samples, from the stream's length (never decoded)
            const double samples = double(std::max(im.pixelWidth, 0)) * double(std::max(im.pixelHeight, 0));
            if (!im.info.inlineImg && im.info.encodedBytes >= 0 && samples > 0) {
                j.kmeasure("bytesPerPixel", double(im.info.encodedBytes) / samples);
            }
            j.close();
        }
        j.close(']');
        j.kbool("itemsTruncated", images.size() > maxItems);
    }
    j.kbool("truncated", text.getImagesPageTruncated());
    j.close();
    if (text.getImagesPageTruncated()) {
        s.warnings |= 1u << wLimit;
    }

    // D: vectors
    auto opIndex = [](const char *name) {
        for (int i = 0; i < Gfx::opCount(); ++i) {
            if (!strcmp(Gfx::opName(i), name)) {
                return i;
            }
        }
        return -1;
    };
    auto shown = [&](const char *name) {
        const int i = opIndex(name);
        uint64_t n = 0;
        if (i >= 0) {
            add(n, tally.visible[i]);
            add(n, tally.inLayer[i]);
        }
        return n;
    };
    if (wants(Vectors)) {
        uint64_t hRules = 0, vRules = 0;
        std::vector<const TextRule *> hs, vs;
        for (const TextRule &r : text.getRules()) {
            if (r.box || r.imageMask) {
                continue;
            }
            if (r.xMax - r.xMin >= r.yMax - r.yMin) {
                ++hRules;
                if (hs.size() < 512) {
                    hs.push_back(&r);
                }
            } else {
                ++vRules;
                if (vs.size() < 512) {
                    vs.push_back(&r);
                }
            }
        }
        // grid score: the share of (row, column) line pairs that cross, over the distinct rows
        // and columns (1 point apart); a full ruled table scores 1
        double gridScore = 0;
        {
            std::vector<double> ys, xs;
            for (const TextRule *r : hs) {
                ys.push_back(std::round(0.5 * (r->yMin + r->yMax)));
            }
            for (const TextRule *r : vs) {
                xs.push_back(std::round(0.5 * (r->xMin + r->xMax)));
            }
            std::sort(ys.begin(), ys.end());
            ys.erase(std::unique(ys.begin(), ys.end()), ys.end());
            std::sort(xs.begin(), xs.end());
            xs.erase(std::unique(xs.begin(), xs.end()), xs.end());
            if (ys.size() >= 2 && xs.size() >= 2) {
                gridScore = double(countRuleCrossings(hs, vs, ys, xs)) / (double(ys.size()) * double(xs.size()));
            }
        }
        bool graphicsLimit = false;
        std::vector<TextGraphic> clusters = text.getGraphics(3.0, &graphicsBudget, &graphicsLimit);
        // Bar charts drawn only with filled boxes (doppler#38), after the drawing clusters (their list order is unchanged);
        // a bar cluster overlapping a drawing cluster is joined into it. Its own per-page budget (never graphicsBudget).
        std::sort(s.wordCentres.begin(), s.wordCentres.end(), [](const WordCentre &a, const WordCentre &b) { return a.x < b.x; });
        if (s.barWordsTruncated) {
            graphicsLimit = true; // a page over kMaxBarWords: the text test saw only the first words
        }
        for (const TextGraphic &g : barClusters(text.getRules(), s.wordCentres, s.scaleNumbers, pageW, pageH, clusters, &barBudget, &graphicsLimit)) {
            clusters.push_back(g);
        }
        const int vectorCells = s.vectorGrid.count();
        j.kopen("vectors");
        j.kint("paths", s.paths);
        j.kint("filled", s.filled);
        j.kint("stroked", s.stroked);
        j.kratio("coverage", s.vectorGrid.share());
        if (vectorCells > 0) {
            j.kratio("uncoveredRatio", double(s.vectorGrid.countAndNot(s.textGrid)) / vectorCells);
        }
        j.kint("hRules", hRules);
        j.kint("vRules", vRules);
        j.kratio("ruleGridScore", gridScore);
        uint64_t shadings = shown("sh"), clips = shown("W");
        add(shadings, tally.shadingPatternPaints);
        add(clips, shown("W*"));
        j.kint("shadings", shadings);
        j.kint("patterns", tally.tilingPatternPaints);
        j.kint("clips", clips);
        j.kint("figureClusters", clusters.size());
        j.kopen("clusters", '[');
        for (size_t i = 0; i < clusters.size() && i < maxItems; ++i) {
            const TextGraphic &g = clusters[i];
            j.open();
            j.kbox("bbox", g.xMin, g.yMin, g.xMax, g.yMax);
            j.kint("paths", g.paths);
            j.kint("curves", g.curves);
            j.kint("segments", g.segments);
            j.close();
        }
        j.close(']');
        j.kbool("clustersTruncated", clusters.size() > maxItems);
        double body = 10.0; // pdftomarkdown's default without text
        uint64_t bodyChars = 0;
        for (int bin : s.sizeOrder) {
            if (s.sizeChars[bin] > bodyChars) {
                body = bin / 2.0;
                bodyChars = s.sizeChars[bin];
            }
        }
        bool gridsTruncated = false;
        const std::vector<RuleGrid> grids = ruleGrids(text.getRules(), pageW, pageH, body, &gridsTruncated);
        // the grids came from a partial rule list (the page's rule cap), or from a body size
        // that ignored words of a size outside the bins: incomplete, said so
        gridsTruncated = gridsTruncated || text.getRulesPageTruncated() || s.sizeOutOfRange;
        // each grid's thin rules (thinRuleGrids), from the rules at the 0.01 pt the XHTML
        // writes (rounded as its %.2f is); absent from every grid (with unlistedRuledGrids, and
        // gridsTruncated set) when the page's rule list is partial, the table was rebuilt, the walker would read fewer
        // rules than the page has (past walkerMaxRules, or past the run's rule budget), a thin
        // rule's extent or centre is not finite, more than maxGridRules thin rules run either
        // way, or the maxGridWork visits run out: counts from part of the rules could be wrong
        std::vector<ThinRule> th, tv;
        // a cross-reference table rebuilt after open (by the doc line's reads, or by any fetch
        // before this point) can make an object resolve to another body than a run that never
        // rebuilt it sees, so the rules here may not be the ones such a run writes; a rebuild at
        // open is every run's. Without the doc line (never: both callers write it) fail closed.
        bool thinComplete = !text.getRulesPageTruncated() && docXref && docXref->getReconstructionCount() == openReconstructions;
        if (thinComplete) {
            auto at01 = [](double v) {
                // what printf's %.2f writes, read back (both round the exact binary value)
                char buf[400];
                const auto [end, ec] = std::to_chars(buf, buf + sizeof(buf), v, std::chars_format::fixed, 2);
                double back = NAN;
                if (ec == std::errc()) {
                    std::from_chars(buf, end, back);
                }
                return back;
            };
            // the walker reads the <rule>s the XHTML writes, at most walkerMaxRules a page: on a
            // page with more non-box rules, or with any past the run's rule budget (which the
            // XHTML leaves out), it sees fewer than these facts would count
            size_t written = 0;
            for (const TextRule &r : text.getRules()) {
                if (r.box) {
                    continue;
                }
                if (r.pastDocumentCap || ++written > walkerMaxRules) {
                    thinComplete = false;
                    break;
                }
                ThinRule k { at01(r.xMin), at01(r.yMin), at01(r.xMax), at01(r.yMax), 0 };
                if (!std::isfinite(k.x0) || !std::isfinite(k.y0) || !std::isfinite(k.x1) || !std::isfinite(k.y1)) {
                    thinComplete = false; // never skipped: the walker would read it
                    break;
                }
                const double w = k.x1 - k.x0, hh = k.y1 - k.y0;
                std::vector<ThinRule> *into = nullptr;
                if (hh <= thinRuleMaxPt && w >= thinRuleMinPt) {
                    k.c = 0.5 * (k.y0 + k.y1);
                    into = &th;
                } else if (w <= thinRuleMaxPt && hh >= thinRuleMinPt) {
                    k.c = 0.5 * (k.x0 + k.x1);
                    into = &tv;
                }
                if (!into) {
                    continue;
                }
                if (!std::isfinite(w) || !std::isfinite(hh) || !std::isfinite(k.c) || std::fabs(k.c) > maxPoint) {
                    thinComplete = false; // an extent or centre past finite range: fail closed, never skip
                    break;
                }
                if (into->size() >= maxGridRules) {
                    thinComplete = false;
                    break;
                }
                into->push_back(k);
            }
            std::ranges::stable_sort(th, {}, &ThinRule::c);
            std::ranges::stable_sort(tv, {}, &ThinRule::c);
        }
        std::vector<GridRules> perGrid;
        uint64_t unlisted = 0;
        thinComplete = thinComplete && thinRuleGrids(grids, th, tv, &perGrid, &unlisted);
        gridsTruncated = gridsTruncated || !thinComplete;
        j.kopen("grids", '[');
        for (size_t i = 0; i < grids.size(); ++i) {
            const RuleGrid &g = grids[i];
            j.open();
            j.kbox("bbox", g.x0, g.y0, g.x1, g.y1);
            j.kint("rows", g.rows);
            j.kint("columns", g.columns);
            j.kratio("coverage", area > 0 ? (g.x1 - g.x0) * (g.y1 - g.y0) / area : 0.0);
            if (thinComplete && perGrid[i].known) {
                j.kint("hRules", perGrid[i].hRules);
                j.kint("vRules", perGrid[i].vRules);
                j.kint("outerSides", perGrid[i].outerSides);
                j.kbool("connected", perGrid[i].connected);
            }
            j.close();
        }
        j.close(']');
        j.kbool("gridsTruncated", gridsTruncated);
        if (thinComplete) {
            j.kint("unlistedRuledGrids", unlisted);
        }
        const bool limit = s.pathLimit || text.getRulesPageTruncated() || graphicsLimit;
        j.kbool("limitHit", limit);
        j.close();
        if (limit) {
            features |= 1u << fDrawingLimit;
            s.warnings |= 1u << wLimit;
        }
    }

    // E, F: forms and annotations
    if (wants(Forms | Annots) && s.page) {
        ::Annots *annots = s.page->getAnnots();
        static constexpr const char *subtypeNames[] = { "text",  "link", "freeText", "line",           "square", "circle", "polygon", "polyLine", "highlight",   "underline", "squiggly",  "strikeOut", "stamp",
                                                        "caret", "ink",  "popup",    "fileAttachment", "sound",  "movie",  "widget",  "screen",   "printerMark", "trapNet",   "watermark", "threeD",    "richMedia" };
        std::array<uint64_t, 27> bySubtype {};
        uint64_t total = 0, visible = 0, linkUri = 0, linkInternal = 0, linkOther = 0;
        uint64_t fText = 0, fCheck = 0, fRadio = 0, fChoice = 0, fSig = 0, fPush = 0, filledN = 0, emptyN = 0, mismatch = 0, signedN = 0, unsignedN = 0;
        bool truncatedAnnots = false, formsTruncated = false;
        size_t drawCursor = 0;
        if (docXref && s.formObjectsGeneration != docXref->getGeneration()) {
            s.formObjects = FormCache();
            s.formObjectsGeneration = docXref->getGeneration();
        }
        FormObjects formObjects(s.formObjects, docXref);
        if (annots) {
            for (const std::shared_ptr<Annot> &a : annots->getAnnots()) {
                if (total >= maxAnnots) {
                    truncatedAnnots = true;
                    break;
                }
                ++total;
                const int type = static_cast<int>(a->getType());
                ++bySubtype[type >= 0 && type <= 26 ? type : 0];
                // Page::displaySlice draws the annotations in this order: walk the records along
                const AnnotDraw *draw = nullptr;
                if (drawCursor < s.draws.size() && s.draws[drawCursor].annot == a.get()) {
                    draw = &s.draws[drawCursor++];
                }
                if (draw && draw->marks > 0) {
                    ++visible;
                }
                if (a->getType() == Annot::typeLink) {
                    const LinkAction *action = static_cast<AnnotLink *>(a.get())->getAction();
                    const LinkActionKind kind = action ? action->getKind() : actionUnknown;
                    if (kind == actionURI) {
                        ++linkUri;
                    } else if (kind == actionGoTo || kind == actionNamed) {
                        ++linkInternal;
                    } else {
                        ++linkOther;
                    }
                }
                if (a->getType() == Annot::typeWidget && wants(Forms)) {
                    // past the document's form budget a widget is not read, and not counted
                    const WidgetField w = formObjects.exhausted() ? WidgetField() : widgetField(a->getAnnotObj(), formObjects);
                    if (formObjects.exhausted()) {
                        formsTruncated = true;
                        continue;
                    }
                    const bool shows = draw && draw->glyphs > 0;
                    switch (w.kind) {
                    case WidgetField::Text:
                    case WidgetField::Choice:
                        ++(w.kind == WidgetField::Text ? fText : fChoice);
                        ++(w.filled ? filledN : emptyN);
                        mismatch += w.filled != shows;
                        break;
                    case WidgetField::Checkbox:
                    case WidgetField::Radio:
                        ++(w.kind == WidgetField::Checkbox ? fCheck : fRadio);
                        ++(w.filled ? filledN : emptyN);
                        mismatch += w.filled != w.appearanceOn;
                        break;
                    case WidgetField::Signature:
                        ++fSig;
                        ++(w.filled ? signedN : unsignedN);
                        break;
                    case WidgetField::PushButton:
                        ++fPush;
                        break;
                    case WidgetField::Unknown:
                        break;
                    }
                }
            }
        }
        if (wants(Forms)) {
            j.kopen("forms");
            j.kint("fields", fText + fCheck + fRadio + fChoice + fSig + fPush);
            j.kint("text", fText);
            j.kint("checkbox", fCheck);
            j.kint("radio", fRadio);
            j.kint("choice", fChoice);
            j.kint("signature", fSig);
            j.kint("pushButton", fPush);
            j.kint("filled", filledN);
            j.kint("empty", emptyN);
            j.kint("valueAppearanceMismatch", mismatch);
            j.kint("signed", signedN);
            j.kint("unsigned", unsignedN);
            j.kbool("truncated", formsTruncated);
            j.close();
            if (formsTruncated) {
                s.warnings |= 1u << wLimit;
            }
            if (filledN > 0) {
                features |= 1u << fAcroformFilled;
            }
            if (emptyN > 0) {
                features |= 1u << fAcroformEmpty;
            }
        }
        if (wants(Annots)) {
            j.kopen("annots");
            j.kint("count", total);
            j.kopen("bySubtype");
            for (int i = 1; i <= 26; ++i) {
                if (bySubtype[i] > 0) {
                    j.kint(subtypeNames[i - 1], bySubtype[i]);
                }
            }
            if (bySubtype[0] > 0) {
                j.kint("other", bySubtype[0]);
            }
            j.close();
            j.kint("linkUri", linkUri);
            j.kint("linkInternal", linkInternal);
            j.kint("linkOther", linkOther);
            j.kint("visibleAppearance", visible);
            j.kbool("truncated", truncatedAnnots);
            j.close();
            if (bySubtype[Annot::typeText] > 0) {
                features |= 1u << fAnnotText;
            }
            if (bySubtype[Annot::typeFreeText] > 0) {
                features |= 1u << fAnnotFreeText;
            }
            if (bySubtype[Annot::typeHighlight] > 0) {
                features |= 1u << fAnnotHighlight;
            }
            for (int i = 0; i <= 26; ++i) {
                if (bySubtype[i] > 0 && i != Annot::typeText && i != Annot::typeFreeText && i != Annot::typeHighlight && i != Annot::typeLink && i != Annot::typeWidget && i != Annot::typePopup) {
                    features |= 1u << fAnnotOther;
                }
            }
            if (linkUri > 0) {
                features |= 1u << fLinkUri;
            }
            if (linkInternal > 0) {
                features |= 1u << fLinkInternal;
            }
            if (truncatedAnnots) {
                s.warnings |= 1u << wLimit;
            }
        }
    }

    // G: optional content
    if (wants(Ocg)) {
        static const char *const contentOps[] = { "S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "sh", "Do", "BI", "Tj", "TJ", "'", "\"" };
        uint64_t hidden = tally.layerXObjectsHidden, inLayer = tally.layerXObjectsShown;
        for (const char *name : contentOps) {
            const int i = opIndex(name);
            if (i >= 0) {
                add(hidden, tally.hidden[i]);
                add(inLayer, tally.inLayer[i]);
            }
        }
        j.kopen("layers");
        j.kbool("hiddenContent", hidden > 0);
        j.kbool("visibleContent", inLayer > 0);
        j.kint("hiddenOps", hidden);
        j.kint("visibleOps", inLayer);
        j.close();
        if (hidden > 0) {
            features |= 1u << fOcgHidden;
        }
        if (inLayer > 0) {
            features |= 1u << fOcgVisible;
        }
    }

    s.body = std::move(j.s);
    s.features = features;
}

std::string PageFacts::finishPage(int pageNum, double ms)
{
    State &s = *st;
    // an earlier page of the run rebuilt the table: an object with two bodies in the file can
    // now resolve to the other one, so this page's facts may differ from a run of it alone
    if (s.reconstructionsAtBegin > docReconstructions) {
        s.warnings |= 1u << wXrefRepairedEarlier;
    }
    if (docXref && docXref->getReconstructionCount() != s.reconstructionsAtBegin) {
        s.warnings |= 1u << wXrefRepaired;
    }
    const bool ok = s.begun && s.ended && s.pageNum == pageNum;
    Json j;
    writeHeader(j, "page");
    j.kint("page", static_cast<uint64_t>(std::max(pageNum, 0)));
    if (ok) {
        if (!s.body.empty()) {
            j.s += ',';
            j.s += s.body;
        }
        ++emitted;
    } else {
        j.kenum("error", "failed");
        s.warnings |= 1u << wPageFailed;
        s.features = 0;
        ++failed;
    }
    j.kmask("parseWarnings", s.warnings, warningNames, wXrefRepairedEarlier + 1);
    j.kmask("unusualFeatures", s.features, featureNames, fXfaPresent + 1);
    if (timing) {
        j.kpt("factsMs", ms);
    }
    j.close();
    j.s += '\n';
    s.reset(0);
    activeFacts = nullptr;
    return j.s;
}

std::string PageFacts::runLine(const char *version, const char *commit, double totalMs, const char *stop) const
{
    Json j;
    writeHeader(j, "run");
    j.kint("pagesEmitted", static_cast<uint64_t>(emitted));
    j.kint("pagesFailed", static_cast<uint64_t>(failed));
    if (stop) {
        j.kenum("stop", stop);
    }
    // version and commit come from the build (digits, letters, '.', '-', '+'); anything else is dropped
    auto clean = [](const char *in) {
        std::string out;
        for (const char *p = in ? in : ""; *p && out.size() < 64; ++p) {
            if (isalnum(static_cast<unsigned char>(*p)) || *p == '.' || *p == '-' || *p == '+' || *p == '_') {
                out += *p;
            }
        }
        return out;
    };
    j.kenum("version", clean(version).c_str());
    j.kenum("commit", clean(commit).c_str());
    if (timing) {
        j.kpt("totalMs", totalMs);
        long long rssKiB = 0;
#ifndef _WIN32
        struct rusage usage;
        if (getrusage(RUSAGE_SELF, &usage) == 0) {
            rssKiB = usage.ru_maxrss;
        }
#endif
        j.kint("peakRssKiB", static_cast<uint64_t>(std::max(rssKiB, 0LL)));
    }
    j.close();
    j.s += '\n';
    return j.s;
}

//------------------------------------------------------------------------
// ImageInfoCache
//------------------------------------------------------------------------

namespace {

ImageColor colorOfName(const std::string &n)
{
    if (n == "DeviceGray" || n == "G" || n == "CalGray") {
        return ImageColor::Gray;
    }
    if (n == "DeviceRGB" || n == "RGB" || n == "CalRGB") {
        return ImageColor::RGB;
    }
    if (n == "DeviceCMYK" || n == "CMYK") {
        return ImageColor::CMYK;
    }
    if (n == "Indexed" || n == "I") {
        return ImageColor::Indexed;
    }
    if (n == "Lab") {
        return ImageColor::Lab;
    }
    if (n == "Separation") {
        return ImageColor::Separation;
    }
    if (n == "DeviceN") {
        return ImageColor::DeviceN;
    }
    return ImageColor::Other;
}

bool isDeviceName(const std::string &n)
{
    return n == "DeviceGray" || n == "DeviceRGB" || n == "DeviceCMYK";
}

ImageInfoCache::Space spaceOfName(const std::string &n)
{
    return { colorOfName(n), n == "CalGray" || n == "CalRGB", isDeviceName(n) };
}

} // namespace

void ImageInfoCache::sync(XRef *xref)
{
    if (xref != docXref || (xref && xref->getGeneration() != generation)) {
        images.clear();
        spaces.clear();
        families.clear();
        profiles.clear();
        docXref = xref;
        generation = xref ? xref->getGeneration() : 0;
        suppressedAtSync = xref ? xref->getSuppressedRepairs() : 0;
    }
}

bool ImageInfoCache::untainted(XRef *xref) const
{
    return !xref || xref->getSuppressedRepairs() == suppressedAtSync;
}

const ImageInfoCache::Image *ImageInfoCache::find(Ref ref, XRef *xref)
{
    sync(xref);
    const auto it = images.find(ref);
    return it == images.end() ? nullptr : &it->second;
}

void ImageInfoCache::store(Ref ref, XRef *xref, const Image &image)
{
    sync(xref);
    if (images.size() < maxEntries && untainted(xref)) {
        images.emplace(ref, image);
    }
}

bool ImageInfoCache::takeFetch()
{
    if (fetchesLeft == 0) {
        ++refused;
        return false;
    }
    --fetchesLeft;
    return true;
}

ImageInfoCache::Space ImageInfoCache::colorSpace(const Object &cs, XRef *xref)
{
    sync(xref);
    if (!cs.isRef()) {
        return classify(cs, xref);
    }
    const auto it = spaces.find(cs.getRef());
    if (it != spaces.end()) {
        return it->second;
    }
    if (!xref || !takeFetch()) {
        return { ImageColor::Other, false, false };
    }
    // classify() only reads the array's first two entries (by reference): a large lookup
    // string is parsed here once and never copied
    const size_t before = refused;
    const Object resolved = xref->fetch(cs.getRef());
    const Space c = classify(resolved, xref);
    // a result shaped by a refused lookup is not kept: it would answer later lookups as limited
    // without saying so (and a retry past the budget is refused at once)
    if (refused == before && spaces.size() < maxEntries && untainted(xref)) {
        spaces.emplace(cs.getRef(), c);
    }
    return c;
}

ImageInfoCache::Space ImageInfoCache::classify(const Object &cs, XRef *xref)
{
    if (cs.isNull()) {
        return { ImageColor::None, false, false };
    }
    if (cs.isName()) {
        return spaceOfName(cs.getNameString());
    }
    if (!cs.isArray() || cs.arrayGetLength() < 1) {
        return { ImageColor::Other, false, false };
    }
    // the family, by name (None stands for ICCBased, which colorOfName never returns)
    const Object &family = cs.getArray()->getNF(0);
    Space c { ImageColor::Other, false, false };
    if (family.isName()) {
        c = family.isName("ICCBased") ? Space { ImageColor::None, false, false } : spaceOfName(family.getNameString());
    } else if (family.isRef()) {
        c = familyOf(family, xref);
    }
    if (c.color == ImageColor::None) {
        return { cs.arrayGetLength() >= 2 ? iccOf(cs.getArray()->getNF(1), xref) : ImageColor::Other, false, false };
    }
    // a device space only as a bare name; an array is one only as [/Indexed /Device... ...],
    // both names given directly
    c.device = false;
    if (family.isName("Indexed") && cs.arrayGetLength() >= 2) {
        const Object &base = cs.getArray()->getNF(1);
        c.device = base.isName() && isDeviceName(base.getNameString());
    }
    return c;
}

// An indirect family name: its colour (None for ICCBased), once per reference.
ImageInfoCache::Space ImageInfoCache::familyOf(const Object &family, XRef *xref)
{
    const auto it = families.find(family.getRef());
    if (it != families.end()) {
        return it->second;
    }
    if (!xref || !takeFetch()) {
        return { ImageColor::Other, false, false };
    }
    const Object name = xref->fetch(family.getRef());
    const Space c = !name.isName() ? Space { ImageColor::Other, false, false } : name.isName("ICCBased") ? Space { ImageColor::None, false, false } : spaceOfName(name.getNameString());
    if (families.size() < maxEntries && untainted(xref)) {
        families.emplace(family.getRef(), c);
    }
    return c;
}

ImageColor ImageInfoCache::iccOf(const Object &profile, XRef *xref)
{
    // the profile is a stream, so indirect
    if (!profile.isRef()) {
        return ImageColor::Other;
    }
    const auto it = profiles.find(profile.getRef());
    if (it != profiles.end()) {
        return it->second;
    }
    if (!xref || !takeFetch()) {
        return ImageColor::Other;
    }
    ImageColor c = ImageColor::Other;
    const Object icc = xref->fetch(profile.getRef());
    const size_t before = refused;
    if (icc.isStream()) {
        // /N by reference is resolved too, within the same budget
        const Object &nRaw = icc.getStream()->getDict()->lookupNF("N");
        Object n;
        if (nRaw.isRef()) {
            if (takeFetch()) {
                n = xref->fetch(nRaw.getRef());
            }
        } else {
            n = nRaw.copy();
        }
        if (n.isInt()) {
            switch (n.getInt()) {
            case 1:
                c = ImageColor::ICC1;
                break;
            case 3:
                c = ImageColor::ICC3;
                break;
            case 4:
                c = ImageColor::ICC4;
                break;
            default:
                break;
            }
        }
    }
    if (refused == before && profiles.size() < maxEntries && untainted(xref)) {
        profiles.emplace(profile.getRef(), c);
    }
    return c;
}
