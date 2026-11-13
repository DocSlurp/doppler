//========================================================================
//
// pdftotext.cc
//
// Copyright 1997-2003 Glyph & Cog, LLC
//
// Modified for Debian by Hamish Moffatt, 22 May 2002.
//
//========================================================================

//========================================================================
//
// Modified under the Poppler project - http://poppler.freedesktop.org
//
// All changes made under the Poppler project to this file are licensed
// under GPL version 2 or later
//
// Copyright (C) 2006 Dominic Lachowicz <cinamod@hotmail.com>
// Copyright (C) 2007-2008, 2010, 2011, 2017-2022, 2024-2026 Albert Astals Cid <aacid@kde.org>
// Copyright (C) 2009 Jan Jockusch <jan@jockusch.de>
// Copyright (C) 2010, 2013 Hib Eris <hib@hiberis.nl>
// Copyright (C) 2010 Kenneth Berland <ken@hero.com>
// Copyright (C) 2011 Tom Gleason <tom@buildadam.com>
// Copyright (C) 2011 Steven Murdoch <Steven.Murdoch@cl.cam.ac.uk>
// Copyright (C) 2013 Yury G. Kudryashov <urkud.urkud@gmail.com>
// Copyright (C) 2013 Suzuki Toshiya <mpsuzuki@hiroshima-u.ac.jp>
// Copyright (C) 2015 Jeremy Echols <jechols@uoregon.edu>
// Copyright (C) 2017 Adrian Johnson <ajohnson@redneon.com>
// Copyright (C) 2018 Klarälvdalens Datakonsult AB, a KDAB Group company, <info@kdab.com>. Work sponsored by the LiMux project of the city of Munich
// Copyright (C) 2018 Adam Reichold <adam.reichold@t-online.de>
// Copyright (C) 2018 Sanchit Anand <sanxchit@gmail.com>
// Copyright (C) 2019 Dan Shea <dan.shea@logical-innovations.com>
// Copyright (C) 2019, 2021 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright (C) 2021 William Bader <williambader@hotmail.com>
// Copyright (C) 2022 kVdNi <kVdNi@waqa.eu>
// Copyright (C) 2025, 2026 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright (C) 2025 Hagen Möbius <hagen.moebius@googlemail.com>
// Copyright (C) 2026 Ojas Maheshwari <workonlyojas@gmail.com>
// Copyright (C) 2026 Aditya Tiwari <suntiwari3495@gmail.com>
// Copyright (C) 2026 Diego Costa R. <diegocostar@gmail.com>
//
// To see a description of the changes please see the Changelog file that
// came with your tarball or type make ChangeLog if you are building from git
//
//========================================================================

#include "config.h"
#include <poppler-config.h>
#include <bit>
#include <cstdio>
#include <cstdlib>
#include <cstddef>
#include <cmath>
#include <cstring>
#include "parseargs.h"
#include "printencodings.h"
#include "goo/GooString.h"
#include "GlobalParams.h"
#include "Object.h"
#include "Dict.h"
#include "XRef.h"
#include "Catalog.h"
#include "Page.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "TextOutputDev.h"
#include "CharTypes.h"
#include "UnicodeMap.h"
#include "PDFDocEncoding.h"
#include "Error.h"
#include <algorithm>
#include <string>
#include <sstream>
#include <iomanip>
#include <charconv>
#include "Win32Console.h"
#include "DateInfo.h"
#include "GfxFont.h"
#include "CharCodeToUnicode.h"
#include "Stream.h"
#include <map>
#include <memory>
#include <unordered_map>
#include <set>
#include <vector>
#include <cerrno>
#include <chrono>
#ifndef _WIN32
#    include <fcntl.h>
#    include <csignal>
#    include <pthread.h>
#    include <unistd.h>
#endif
#include "PageFacts.h"
#include "doppler_commit.h"

static void printInfoString(FILE *f, Dict *infoDict, const char *key, const char *text1, const char *text2, const UnicodeMap *uMap);
static void printInfoDate(FILE *f, Dict *infoDict, const char *key, const char *text1, const char *text2);
void printDocBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last);
void printWordBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last);
void printTSVBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last);

static int firstPage = 1;
static int lastPage = 0;
static double resolution = 72.0;
static int x = 0;
static int y = 0;
static int w = 0;
static int h = 0;
static bool bbox = false;
static bool bboxLayout = false;
static bool physLayout = false;
static bool useCropBox = false;
static bool cropSizes = false;
static double colspacing = TextOutputDev::minColSpacing1_default;
static double fixedPitch = 0;
static bool rawOrder = false;
static bool discardDiag = false;
static bool htmlMeta = false;
static char textEncName[128] = "";
static char textEOLStr[16] = "";
static bool noPageBreaks = false;
static bool printURLs = false;
static char ownerPassword[128] = "\001";
static char userPassword[128] = "\001";
static bool quiet = false;
static bool printVersion = false;
static bool printHelp = false;
static bool printEnc = false;
static bool tsvMode = false;
static bool wordMeta = false;
static bool collectRules = false;
static bool collectImages = false;
static bool compactLayout = false;
static bool glyphRecords = false;
static char hyphenModeStr[16] = "";
static int pageFactsFd = -1;
// -timeout S, -timeout-per-page P, -timeout-max M: the run's wall-clock limit is S seconds plus
// P per page of the range (counted once the document is open), at most M (0: no cap); exit
// code 6 past it. All 0 (the default): no limit.
static char timeoutArg[16] = "0";
static char timeoutPerPageArg[16] = "0";
static char timeoutMaxArg[16] = "0";
static char pageFactsLevel[16] = "basic";
static char pageFactsInclude[128] = "";
static bool pageFactsTiming = false;
static int scanMaxWords = static_cast<int>(ScanThresholds::defaultMaxVisibleWords);
static char scanMinCoverageArg[32] = "0.85";
static char scanLayerOverlapArg[32] = "0.5";
static ScanThresholds scanThresholds; // from the three -scan-* options

static const ArgDesc argDesc[] = {
    { .arg = "-f", .kind = argInt, .val = &firstPage, .size = 0, .usage = "first page to convert" },
    { .arg = "-l", .kind = argInt, .val = &lastPage, .size = 0, .usage = "last page to convert" },
    { .arg = "-r", .kind = argFP, .val = &resolution, .size = 0, .usage = "resolution, in DPI (default is 72)" },
    { .arg = "-x", .kind = argInt, .val = &x, .size = 0, .usage = "x-coordinate of the crop area top left corner" },
    { .arg = "-y", .kind = argInt, .val = &y, .size = 0, .usage = "y-coordinate of the crop area top left corner" },
    { .arg = "-W", .kind = argInt, .val = &w, .size = 0, .usage = "width of crop area in pixels (default is 0)" },
    { .arg = "-H", .kind = argInt, .val = &h, .size = 0, .usage = "height of crop area in pixels (default is 0)" },
    { .arg = "-layout", .kind = argFlag, .val = &physLayout, .size = 0, .usage = "maintain original physical layout" },
    { .arg = "-fixed", .kind = argFP, .val = &fixedPitch, .size = 0, .usage = "assume fixed-pitch (or tabular) text" },
    { .arg = "-raw", .kind = argFlag, .val = &rawOrder, .size = 0, .usage = "keep strings in content stream order" },
    { .arg = "-remove-hyphens", .kind = argString, .val = hyphenModeStr, .size = sizeof(hyphenModeStr), .usage = "end-of-line hyphen handling: none, soft, or all (default: all)" },
    { .arg = "-nodiag", .kind = argFlag, .val = &discardDiag, .size = 0, .usage = "discard diagonal text" },
    { .arg = "-htmlmeta", .kind = argFlag, .val = &htmlMeta, .size = 0, .usage = "generate a simple HTML file, including the meta information" },
    { .arg = "-tsv", .kind = argFlag, .val = &tsvMode, .size = 0, .usage = "generate a simple TSV file, including the meta information for bounding boxes" },
    { .arg = "-wordmeta", .kind = argFlag, .val = &wordMeta, .size = 0, .usage = "with -bbox, -bbox-layout or -tsv: also output per-word font, size, rotation, space-after, invisible, colour and baseline" },
    { .arg = "-glyphs", .kind = argFlag, .val = &glyphRecords, .size = 0, .usage = "with -bbox-layout -wordmeta: also output font tables, glyph records and (with -rules) image-mask rules" },
    { .arg = "-compact-layout", .kind = argFlag, .val = &compactLayout, .size = 0, .usage = "with -bbox-layout: fast locale-independent word coordinates at 0.001-point precision" },
    { .arg = "-enc", .kind = argString, .val = textEncName, .size = sizeof(textEncName), .usage = "output text encoding name" },
    { .arg = "-listenc", .kind = argFlag, .val = &printEnc, .size = 0, .usage = "list available encodings" },
    { .arg = "-eol", .kind = argString, .val = textEOLStr, .size = sizeof(textEOLStr), .usage = "output end-of-line convention (unix, dos, or mac)" },
    { .arg = "-nopgbrk", .kind = argFlag, .val = &noPageBreaks, .size = 0, .usage = "don't insert page breaks between pages" },
    { .arg = "-urls", .kind = argFlag, .val = &printURLs, .size = 0, .usage = "print the URL of each link next to its link text" },
    { .arg = "-bbox", .kind = argFlag, .val = &bbox, .size = 0, .usage = "output bounding box for each word and page size to html. Sets -htmlmeta" },
    { .arg = "-rules", .kind = argFlag, .val = &collectRules, .size = 0, .usage = "with -bbox-layout: also output axis-aligned vector rules and filled boxes (table borders, shading)" },
    { .arg = "-images",
      .kind = argFlag,
      .val = &collectImages,
      .size = 0,
      .usage = "with -bbox-layout: also output where each visible image and image mask lands, clusters of drawing that is not axis-aligned (charts, diagrams), and the page number (image data is never decoded)" },
    { .arg = "-bbox-layout", .kind = argFlag, .val = &bboxLayout, .size = 0, .usage = "like -bbox but with extra layout bounding box data.  Sets -htmlmeta" },
    { .arg = "-cropbox", .kind = argFlag, .val = &useCropBox, .size = 0, .usage = "use the crop box rather than media box" },
    { .arg = "-crop-sizes", .kind = argFlag, .val = &cropSizes, .size = 0, .usage = "with -bbox-layout: also output each page's crop box size as pdfinfo prints it (crop=\"W H\")" },
    { .arg = "-page-facts-fd", .kind = argInt, .val = &pageFactsFd, .size = 0, .usage = "with -bbox-layout: write page facts (JSON lines, doppler-page-facts/1) to this open file descriptor (3 or more)" },
    { .arg = "-page-facts-level", .kind = argString, .val = pageFactsLevel, .size = sizeof(pageFactsLevel), .usage = "page facts level: basic or full (default basic)" },
    { .arg = "-page-facts-include", .kind = argString, .val = pageFactsInclude, .size = sizeof(pageFactsInclude), .usage = "page facts families added to basic: fonts,oddtext,images-detail,vectors,forms,annots,ocg,structure" },
    { .arg = "-page-facts-timing", .kind = argFlag, .val = &pageFactsTiming, .size = 0, .usage = "page facts: add factsMs to page lines and totalMs, peakRssKiB to the run line (not deterministic)" },
    { .arg = "-timeout", .kind = argString, .val = timeoutArg, .size = sizeof(timeoutArg), .usage = "wall-clock limit for the run in seconds, 0 for none (default 0; exit code 6)" },
    { .arg = "-timeout-per-page", .kind = argString, .val = timeoutPerPageArg, .size = sizeof(timeoutPerPageArg), .usage = "seconds added to -timeout per page of the range (default 0)" },
    { .arg = "-timeout-max", .kind = argString, .val = timeoutMaxArg, .size = sizeof(timeoutMaxArg), .usage = "cap on -timeout plus the per-page seconds, 0 for none (default 0)" },
    { .arg = "-scan-max-words", .kind = argInt, .val = &scanMaxWords, .size = 0, .usage = "a page with more visible words is not a scan (page facts and -images scan=; default 20)" },
    { .arg = "-scan-min-coverage", .kind = argString, .val = scanMinCoverageArg, .size = sizeof(scanMinCoverageArg), .usage = "share of the page images must cover for a scan (default 0.85)" },
    { .arg = "-scan-layer-overlap", .kind = argString, .val = scanLayerOverlapArg, .size = sizeof(scanLayerOverlapArg), .usage = "share of the page a layer over a scan must cover (default 0.5)" },
    { .arg = "-colspacing",
      .kind = argFP,
      .val = &colspacing,
      .size = 0,
      .usage = "how much spacing we allow after a word before considering adjacent text to be a new column, as a fraction of the font size (default is 0.7, old releases had a 0.3 default)" },
    { .arg = "-opw", .kind = argString, .val = ownerPassword, .size = sizeof(ownerPassword), .usage = "owner password (for encrypted files)" },
    { .arg = "-upw", .kind = argString, .val = userPassword, .size = sizeof(userPassword), .usage = "user password (for encrypted files)" },
    { .arg = "-q", .kind = argFlag, .val = &quiet, .size = 0, .usage = "don't print any messages or errors" },
    { .arg = "-v", .kind = argFlag, .val = &printVersion, .size = 0, .usage = "print copyright and version info" },
    { .arg = "-h", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "-help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "--help", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    { .arg = "-?", .kind = argFlag, .val = &printHelp, .size = 0, .usage = "print usage information" },
    {}
};

// Escape text for use in XML element content or attribute values. Besides the
// five predefined entities, control characters (which are either illegal in
// XML 1.0 or, for \t \n \r, would make a word span several lines) and the
// noncharacters U+FFFE/U+FFFF are replaced by a space, so the output is always
// well-formed.
static std::string myXmlTokenReplace(const char *inString)
{
    std::string result;
    result.reserve(strlen(inString) + 8);
    for (const char *p = inString; *p; ++p) {
        const unsigned char c = *p;
        switch (c) {
        case '&':
            result += "&amp;";
            break;
        case '\'':
            result += "&apos;";
            break;
        case '"':
            result += "&quot;";
            break;
        case '<':
            result += "&lt;";
            break;
        case '>':
            result += "&gt;";
            break;
        default:
            if (c < 0x20 || c == 0x7f) {
                result += ' ';
            } else if (c == 0xef && static_cast<unsigned char>(p[1]) == 0xbf && (static_cast<unsigned char>(p[2]) == 0xbe || static_cast<unsigned char>(p[2]) == 0xbf)) {
                result += ' ';
                p += 2;
            } else {
                result += static_cast<char>(c);
            }
        }
    }
    return result;
}

// Make text safe for a single TSV field. Text such as ActualText can contain
// tabs and line breaks, which would split the field or the row, so every ASCII
// control character (including \t \r \n \f \v) and the Unicode line
// separators U+0085, U+2028 and U+2029 (in UTF-8) is replaced by one space.
// Nothing is quoted or backslash-escaped, so ordinary text is unchanged.
static std::string myTsvFieldReplace(const char *inString)
{
    std::string result;
    result.reserve(strlen(inString) + 8);
    for (const char *p = inString; *p; ++p) {
        const unsigned char c = *p;
        if (c < 0x20 || c == 0x7f) {
            result += ' ';
        } else if (c == 0xc2 && static_cast<unsigned char>(p[1]) == 0x85) {
            result += ' ';
            p += 1;
        } else if (c == 0xe2 && static_cast<unsigned char>(p[1]) == 0x80 && (static_cast<unsigned char>(p[2]) == 0xa8 || static_cast<unsigned char>(p[2]) == 0xa9)) {
            result += ' ';
            p += 2;
        } else {
            result += static_cast<char>(c);
        }
    }
    return result;
}

// "#rrggbb" for a colour with components in 0..1
static std::string hexColor(double r, double g, double b)
{
    const auto byte = [](double c) { return static_cast<int>(std::clamp(c, 0.0, 1.0) * 255.0 + 0.5); };
    char buf[8];
    snprintf(buf, sizeof(buf), "#%02x%02x%02x", byte(r), byte(g), byte(b));
    return buf;
}

namespace {

struct TextBounds
{
    double xMin = 0, yMin = 0, xMax = 0, yMax = 0;
};

struct WordMetadata
{
    std::string fontName;
    double fontSize;
    int rotation;
    bool spaceAfter, invisible;
    double r, g, b;
    double baseline;
    bool operator==(const WordMetadata &) const = default;
};

struct ProjectedWord
{
    const TextWord *word = nullptr;
    TextBounds bounds;
    std::unique_ptr<std::string> text;
    std::optional<WordMetadata> metadata;
};

// Private projection module at the extraction-to-writer seam. Writers consume
// word facts without choosing font characters, text direction, or geometry.
// Encoding lookup remains per output run; metadata is only selected on request.
static ProjectedWord projectWord(const TextWord *word, const UnicodeMap *uMap, bool withMetadata)
{
    ProjectedWord result;
    result.word = word;
    word->getBBox(&result.bounds.xMin, &result.bounds.yMin, &result.bounds.xMax, &result.bounds.yMax);
    result.text = word->getLogicalText(uMap);
    if (withMetadata) {
        WordMetadata &meta = result.metadata.emplace();
        const GooString *name = word->getLength() > 0 ? word->getFontName(0) : nullptr;
        meta.fontName = name ? name->toStr() : std::string();
        meta.fontSize = word->getFontSize();
        meta.rotation = word->getRotation();
        meta.spaceAfter = word->hasSpaceAfter();
        meta.invisible = word->isInvisible();
        word->getColor(&meta.r, &meta.g, &meta.b);
        meta.baseline = word->getBaseline();
    }
    return result;
}

// Visit words in extraction order and return their union. A line is projected
// in one pass without retaining a second collection of words. Initialize from
// the first word: zero and negative coordinates are valid bounds, not sentinels.
template<typename WriteWord>
static TextBounds projectLine(const TextLine *line, const UnicodeMap *uMap, bool withMetadata, WriteWord writeWord)
{
    TextBounds bounds;
    bool first = true;
    for (const TextWord *word = line->getWords(); word; word = word->getNext()) {
        const ProjectedWord projected = projectWord(word, uMap, withMetadata);
        if (first) {
            bounds = projected.bounds;
            first = false;
        } else {
            bounds.xMin = std::min(bounds.xMin, projected.bounds.xMin);
            bounds.yMin = std::min(bounds.yMin, projected.bounds.yMin);
            bounds.xMax = std::max(bounds.xMax, projected.bounds.xMax);
            bounds.yMax = std::max(bounds.yMax, projected.bounds.yMax);
        }
        writeWord(projected);
    }
    return bounds;
}

} // namespace

// Appends value as printf("%f") does in the C locale (std::to_chars with precision 6 is exactly
// rounded too, so the text is identical), without printf's multi-precision formatting.
static void appendFixed6(std::string &out, double value)
{
    char buffer[384]; // the fixed representation of the largest finite double fits
    const auto result = std::to_chars(buffer, buffer + sizeof(buffer), value, std::chars_format::fixed, 6);
    if (result.ec == std::errc()) {
        out.append(buffer, result.ptr);
    } else {
        char fallback[400];
        snprintf(fallback, sizeof(fallback), "%f", value);
        out += fallback;
    }
}

// -wordmeta: extra attributes of a <word> element, each preceded by a space.
static std::string wordMetaXmlAttributes(const WordMetadata &meta)
{
    std::string tail = " fontSize=\"";
    appendFixed6(tail, meta.fontSize);
    tail += "\" rotation=\"" + std::to_string(meta.rotation) + "\" spaceAfter=\"" + (meta.spaceAfter ? "1" : "0") + "\" invisible=\"" + (meta.invisible ? "1" : "0") + "\" color=\"" + hexColor(meta.r, meta.g, meta.b) + "\" baseline=\"";
    appendFixed6(tail, meta.baseline);
    tail += "\"";
    // as the snprintf into a 256-byte buffer this replaces: huge values are cut at 255 bytes
    if (tail.size() > 255) {
        tail.resize(255);
    }
    return " fontName=\"" + myXmlTokenReplace(meta.fontName.c_str()) + "\"" + tail;
}

// <line> or <block> start tag with printf("%f") coordinates.
static void putBoxTag(FILE *f, const char *open, double xMin, double yMin, double xMax, double yMax)
{
    std::string tag = open;
    tag += " xMin=\"";
    appendFixed6(tag, xMin);
    tag += "\" yMin=\"";
    appendFixed6(tag, yMin);
    tag += "\" xMax=\"";
    appendFixed6(tag, xMax);
    tag += "\" yMax=\"";
    appendFixed6(tag, yMax);
    tag += "\">\n";
    fwrite(tag.data(), 1, tag.size(), f);
}

// -wordmeta: extra TSV columns of a word row (starting with a tab), after the text column.
// Rows of other levels get the same number of empty columns.
static const char tsvWordMetaHeader[] = "\tfont_name\tfont_size\trotation\tspace_after\tinvisible\tcolor\tbaseline";
static const char tsvWordMetaEmpty[] = "\t\t\t\t\t\t\t";

static std::string wordMetaTsvColumns(const WordMetadata &meta)
{
    char buf[128];
    snprintf(buf, sizeof(buf), "\t%.2f\t%d\t%d\t%d\t%s\t%.2f", meta.fontSize, meta.rotation, meta.spaceAfter ? 1 : 0, meta.invisible ? 1 : 0, hexColor(meta.r, meta.g, meta.b).c_str(), meta.baseline);
    return "\t" + myTsvFieldReplace(meta.fontName.c_str()) + buf;
}

// -glyphs: a font table per page and glyph records after each word.
//
// Each <word> on a horizontal line (rotation 0) is followed by one <g> element per glyph, in
// drawing order. Characters that one glyph produced (a ligature, a multi-character ToUnicode
// entry) share a record. Attributes:
//   x, w  left edge and advance width along the line
//   b, z  the glyph's baseline and font size, only when they differ from the word's (poppler
//         merges overlapping words, such as a stacked superscript and subscript)
//   f     the font's id in this page's <font> table (absent when the glyph has no font)
//   c     the font-local character code (the CID for CID fonts)
//   g     the glyph id, for CID TrueType fonts with a CIDToGIDMap
//   u     the glyph's Unicode text as hex code points (at most maxGlyphCodePoints)
//   s     where u came from: t the font's ToUnicode CMap, n the glyph name, c a fallback from
//         the character code, the font's built-in encoding or the CID collection, a the text
//         of an /ActualText span (that record stands for every glyph of the span)
//   n     the glyph name from the font's encoding, only when it says more than u does
// Named glyphs that extraction drops because their Unicode is whitespace or NUL (TeX
// extension glyphs with raw codes 0x00-0x20, say) follow the font table as <xg> records with
// x, w, b, z, f, c, s="c" and n; they belong to no word.
// A page has at most maxPageGlyphs records and the document maxDocGlyphs; past either the
// page is marked glyphsTruncated="1" and its remaining words get no records. The font table
// has at most maxPageFonts entries (fontsTruncated="1"; glyphs of later fonts have no f).
namespace {

constexpr size_t maxPageGlyphs = 100000;
constexpr size_t maxDocGlyphs = 4000000;
constexpr size_t maxPageFonts = 1000;
constexpr size_t maxDocFonts = 64;
constexpr size_t maxCMapBytes = 256 * 1024;
constexpr size_t maxDocCMapBytes = 8 * 1024 * 1024;
constexpr size_t maxDocMetadataBytes = 128 * 1024 * 1024;
// Reserve before parsing: two 65536-entry maps, at most 4096 Unicode strings
// (PSTokenizer tokens cap each at 256 bytes), their indices and vector slack.
constexpr size_t fontMetadataReservation = 2 * 1024 * 1024;
constexpr size_t maxDocSourceEntries = 262144;
constexpr int maxGlyphCodePoints = 32;
constexpr size_t maxGlyphNameBytes = 64;
constexpr size_t maxFontNameBytes = 128;

const char *fontTypeName(GfxFontType t)
{
    switch (t) {
    case fontType1:
        return "Type1";
    case fontType1C:
        return "Type1C";
    case fontType1COT:
        return "Type1COT";
    case fontType3:
        return "Type3";
    case fontTrueType:
        return "TrueType";
    case fontTrueTypeOT:
        return "TrueTypeOT";
    case fontCIDType0:
        return "CIDType0";
    case fontCIDType0C:
        return "CIDType0C";
    case fontCIDType0COT:
        return "CIDType0COT";
    case fontCIDType2:
        return "CIDType2";
    case fontCIDType2OT:
        return "CIDType2OT";
    default:
        return "unknown";
    }
}

std::string capped(const std::string &s, size_t maxBytes)
{
    if (s.size() <= maxBytes) {
        return s;
    }
    size_t n = maxBytes;
    while (n > 0 && (static_cast<unsigned char>(s[n]) & 0xc0) == 0x80) {
        --n; // do not split a UTF-8 sequence
    }
    return s.substr(0, n);
}

// "uniXXXX" or "uXXXX[XX]": names that carry their Unicode value
bool unicodeGlyphName(const char *name)
{
    const size_t n = strlen(name);
    const auto hex = [&](size_t from) {
        for (size_t i = from; i < n; ++i) {
            if (!isxdigit(static_cast<unsigned char>(name[i]))) {
                return false;
            }
        }
        return true;
    };
    return (n >= 7 && !strncmp(name, "uni", 3) && (n - 3) % 4 == 0 && hex(3)) || (n >= 5 && n <= 7 && name[0] == 'u' && hex(1));
}

struct GlyphFont
{
    int id;
    std::string name, type, encoding;
    bool cid, embedded, symbolic, hasToUnicode;
    double fontMatrixScale = 0; // Type 3: |FontMatrix[3]|, else 0
    std::unique_ptr<CharCodeToUnicode> ownToUnicode; // the /ToUnicode CMap alone
    std::unordered_map<CharCode, char> source; // u source per code
    size_t pageMisses = 0; // codes used on this page without a ToUnicode entry
    int pageSeen = -1;
};

class GlyphContext
{
public:
    explicit GlyphContext(XRef *xrefA) : xref(xrefA) { }

    // Before a page is printed: index its fonts and decide how many glyph records it gets.
    void startPage(int page, const TextOutputDev *textOut)
    {
        pageNum = page;
        pageFonts.clear();
        pageFontIds.clear();
        pageCodes.clear();
        fontsTruncated = false;
        truncated = false;
        const size_t docLeft = maxDocGlyphs - std::min(maxDocGlyphs, docGlyphs);
        const size_t limit = std::min(maxPageGlyphs, docLeft);
        pageBudget = 0;
        if (limit == 0 || metadataExhausted) {
            truncated = true;
            fontsTruncated = metadataExhausted;
            return;
        }
        size_t glyphs = 0;
        for (const TextFlow *flow = textOut->getFlows(); flow; flow = flow->getNext()) {
            for (const TextBlock *blk = flow->getBlocks(); blk; blk = blk->getNext()) {
                for (const TextLine *line = blk->getLines(); line; line = line->getNext()) {
                    for (const TextWord *word = line->getWords(); word; word = word->getNext()) {
                        if (word->getRotation() != 0) {
                            continue;
                        }
                        const int n = word->getLength();
                        int codePoints = 0;
                        for (int i = 0; i < n; ++i) {
                            if (i > 0 && word->getCharPosAt(i) == word->getCharPosAt(i - 1) && word->getCharCode(i) == word->getCharCode(i - 1)) {
                                if (++codePoints > maxGlyphCodePoints) {
                                    truncated = true;
                                }
                                continue;
                            }
                            codePoints = 1;
                            if (glyphs == limit) {
                                truncated = true;
                                pageBudget = limit;
                                docGlyphs += limit;
                                return;
                            }
                            ++glyphs;
                            GlyphFont *gf = fontOf(word->getFontInfo(i));
                            if (gf && !word->isActualTextChar(i)) {
                                sourceOf(*gf, word, i);
                            }
                        }
                    }
                }
            }
        }
        for (const TextPage::DroppedGlyph &d : textOut->getDroppedGlyphs()) {
            if (glyphs == limit) {
                truncated = true;
                pageBudget = limit;
                docGlyphs += limit;
                return;
            }
            fontOf(d.font);
            ++glyphs;
        }
        pageBudget = std::min({ glyphs, maxPageGlyphs, docLeft });
        truncated = truncated || fontsTruncated || metadataExhausted || glyphs > pageBudget;
        docGlyphs += pageBudget;
    }

    bool pageTruncated() const { return truncated; }
    bool pageFontsTruncated() const { return fontsTruncated; }

    void writeFontTable(FILE *f) const
    {
        for (const GlyphFont *gf : pageFonts) {
            std::string tu = gf->hasToUnicode ? "1" : "0";
            fprintf(f, "    <font id=\"%d\" name=\"%s\" type=\"%s\" enc=\"%s\" emb=\"%d\" sym=\"%d\" cid=\"%d\" tu=\"%s\" tuMiss=\"%zu\"", gf->id, myXmlTokenReplace(gf->name.c_str()).c_str(), gf->type.c_str(),
                    myXmlTokenReplace(gf->encoding.c_str()).c_str(), gf->embedded ? 1 : 0, gf->symbolic ? 1 : 0, gf->cid ? 1 : 0, tu.c_str(), gf->pageMisses);
            if (gf->fontMatrixScale > 0) {
                fprintf(f, " t3scale=\"%g\"", gf->fontMatrixScale);
            }
            fputs("/>\n", f);
        }
    }

    // Named glyphs that extraction dropped as whitespace, as <xg> records with a baseline
    // and size (they belong to no word).
    void writeDropped(FILE *f, const TextOutputDev *textOut)
    {
        std::string out;
        for (const TextPage::DroppedGlyph &d : textOut->getDroppedGlyphs()) {
            if (pageBudget == 0) {
                truncated = true;
                break;
            }
            --pageBudget;
            GlyphFont *gf = fontOf(d.font);
            out += "    <xg x=\"";
            number(out, d.x);
            out += "\" w=\"";
            number(out, d.advance);
            out += "\" b=\"";
            number(out, d.y);
            out += "\" z=\"";
            number(out, d.fontSize);
            out += '"';
            if (gf) {
                out += " f=\"" + std::to_string(gf->id) + "\"";
            }
            out += " c=\"" + std::to_string(d.code) + "\" s=\"c\"";
            const char *name = glyphName(d.font, d.code);
            if (name && *name) {
                out += " n=\"" + myXmlTokenReplace(capped(name, maxGlyphNameBytes).c_str()) + "\"";
            }
            out += "/>\n";
        }
        fputs(out.c_str(), f);
    }

    // The glyph records of one word, or nothing once the page's budget is used.
    void appendWordGlyphs(std::string &out, const TextWord *word)
    {
        if (word->getRotation() != 0) {
            return;
        }
        const int n = word->getLength();
        int i = 0;
        while (i < n && pageBudget > 0) {
            int j = i + 1;
            while (j < n && word->getCharPosAt(j) == word->getCharPosAt(i) && word->getCharCode(j) == word->getCharCode(i)) {
                ++j;
            }
            --pageBudget;
            const double x0 = word->getEdge(i);
            double advance = 0;
            for (int k = i; k < j; ++k) {
                advance += word->getAdvance(k);
            }
            out += "<g x=\"";
            number(out, x0);
            out += "\" w=\"";
            number(out, advance);
            out += '"';
            // words that poppler merged (stacked scripts) keep each glyph's own baseline and size
            const Matrix &m = word->getTextMat(i);
            const Matrix &m0 = word->getTextMat(0);
            if (std::fabs(m.m[5] - word->getBaseline()) > 0.01) {
                out += " b=\"";
                number(out, m.m[5]);
                out += '"';
            }
            const double scale = std::sqrt(std::fabs(m.m[0] * m.m[3] - m.m[1] * m.m[2]));
            const double scale0 = std::sqrt(std::fabs(m0.m[0] * m0.m[3] - m0.m[1] * m0.m[2]));
            if (scale0 > 0 && std::isfinite(scale) && std::fabs(scale / scale0 - 1) > 0.01) {
                out += " z=\"";
                number(out, word->getFontSize() * scale / scale0);
                out += '"';
            }
            const bool actual = word->isActualTextChar(i);
            const TextFontInfo *info = word->getFontInfo(i);
            GlyphFont *gf = fontOf(info);
            const CharCode code = word->getCharCode(i);
            if (gf) {
                out += " f=\"" + std::to_string(gf->id) + "\"";
            }
            if (!actual) {
                out += " c=\"" + std::to_string(code) + "\"";
                const GfxFont *font = info ? info->getGfxFont() : nullptr;
                if (font && font->isCIDFont()) {
                    const auto *cidFont = static_cast<const GfxCIDFont *>(font);
                    if (code < cidFont->getCIDToGIDLen()) {
                        out += " g=\"" + std::to_string(cidFont->getCIDToGID()[code]) + "\"";
                    }
                }
            }
            out += " u=\"";
            char hex[16];
            for (int k = i; k < j && k - i < maxGlyphCodePoints; ++k) {
                snprintf(hex, sizeof(hex), k > i ? " %x" : "%x", static_cast<unsigned int>(*word->getChar(k)));
                out += hex;
            }
            out += "\" s=\"";
            out += actual ? 'a' : (gf ? sourceOf(*gf, word, i) : 'c');
            out += '"';
            if (!actual) {
                const char *name = glyphName(info, code);
                if (name && *name) {
                    const Unicode named = globalParams->mapNameToUnicodeAll(name);
                    if (j - i != 1 || named == 0 || named != *word->getChar(i)) {
                        out += " n=\"" + myXmlTokenReplace(capped(name, maxGlyphNameBytes).c_str()) + "\"";
                    }
                }
            }
            out += "/>";
            i = j;
        }
    }

private:
    static void number(std::string &out, double v)
    {
        char buffer[384];
        const auto result = std::to_chars(buffer, buffer + sizeof(buffer), v, std::chars_format::fixed, 3);
        if (result.ec == std::errc()) {
            out.append(buffer, result.ptr);
        } else {
            out += "0";
        }
    }

    static const char *glyphName(const TextFontInfo *info, CharCode code)
    {
        const GfxFont *font = info ? info->getGfxFont() : nullptr;
        if (!font || font->isCIDFont() || code > 255) {
            return nullptr;
        }
        return static_cast<const Gfx8BitFont *>(font)->getCharName(static_cast<int>(code));
    }

    char sourceOf(GlyphFont &gf, const TextWord *word, int i)
    {
        const CharCode code = word->getCharCode(i);
        const auto it = gf.source.find(code);
        char src;
        bool first = it == gf.source.end();
        if (!first) {
            src = it->second;
        } else {
            if (sourceEntries >= maxDocSourceEntries || metadataBytes > maxDocMetadataBytes - 128) {
                metadataExhausted = true;
                truncated = true;
                fontsTruncated = true;
                return 'c';
            }
            ++sourceEntries;
            metadataBytes += 128; // source cache node plus page-code accounting
            const Unicode *u = nullptr;
            if (gf.ownToUnicode && gf.ownToUnicode->mapToUnicode(code, &u) > 0) {
                src = 't';
            } else {
                const char *name = glyphName(word->getFontInfo(i), code);
                src = (name && (globalParams->mapNameToUnicodeAll(name) != 0 || unicodeGlyphName(name))) ? 'n' : 'c';
            }
            gf.source.emplace(code, src);
        }
        // count each code once per page
        if (src != 't' && gf.hasToUnicode && pageCodes.emplace(gf.id, code).second) {
            ++gf.pageMisses;
        }
        return src;
    }

    GlyphFont *fontOf(const TextFontInfo *info)
    {
        if (metadataExhausted) {
            fontsTruncated = true;
            truncated = true;
            return nullptr;
        }
        const GfxFont *font = info ? info->getGfxFont() : nullptr;
        if (!font) {
            return nullptr;
        }
        const Ref ref = *font->getID();
        auto it = byRef.find({ ref.num, ref.gen });
        GlyphFont *gf;
        if (pageFonts.size() >= maxPageFonts && (it == byRef.end() || !pageFontIds.count(it->second->id))) {
            fontsTruncated = true;
            truncated = true;
            return nullptr;
        }
        if (it == byRef.end()) {
            if (fonts.size() >= maxDocFonts || metadataBytes > maxDocMetadataBytes - fontMetadataReservation) {
                metadataExhausted = true;
                fontsTruncated = true;
                truncated = true;
                return nullptr;
            }
            metadataBytes += fontMetadataReservation;
            auto made = std::make_unique<GlyphFont>();
            made->id = static_cast<int>(fonts.size());
            const std::optional<std::string> &fullName = font->getName();
            made->name = capped(fullName ? font->getNameWithoutSubsetTag() : std::string(), maxFontNameBytes);
            made->type = fontTypeName(font->getType());
            made->encoding = capped(font->getEncodingName(), maxFontNameBytes);
            made->cid = font->isCIDFont();
            Ref emb;
            made->embedded = font->getEmbeddedFontID(&emb);
            made->symbolic = font->isSymbolic();
            made->hasToUnicode = font->hasToUnicodeCMap();
            if (font->getType() == fontType3) {
                made->fontMatrixScale = std::fabs(font->getFontMatrix()[3]);
            }
            if (made->hasToUnicode && ref.num >= 0) {
                // parse the font's own /ToUnicode again, without the fallbacks GfxFont merged in
                Object fontObj = xref->fetch(ref);
                if (fontObj.isDict()) {
                    Object tuObj = fontObj.dictLookup("ToUnicode");
                    if (tuObj.isStream()) {
                        std::string buf;
                        const size_t remaining = maxDocCMapBytes - cmapBytes;
                        const size_t cap = std::min(maxCMapBytes, remaining);
                        if (cap == 0 || mappingsLeft == 0) {
                            metadataExhausted = true;
                            fontsTruncated = true;
                            truncated = true;
                            return nullptr;
                        }
                        Stream *stream = tuObj.getStream();
                        if (!stream->rewind()) {
                            stream->close();
                            metadataExhausted = true;
                            fontsTruncated = true;
                            truncated = true;
                            return nullptr;
                        }
                        int c = EOF;
                        while (buf.size() < cap && (c = stream->getChar()) != EOF) {
                            buf.push_back(static_cast<char>(c));
                        }
                        const bool overflow = buf.size() == cap && stream->getChar() != EOF;
                        stream->close();
                        cmapBytes += buf.size();
                        if (overflow) {
                            metadataExhausted = true;
                            fontsTruncated = true;
                            truncated = true;
                            return nullptr;
                        }
                        made->ownToUnicode = CharCodeToUnicode::parseCMapLimited(buf, made->cid ? 16 : 8, mappingsLeft);
                        if (!made->ownToUnicode) {
                            metadataExhausted = true;
                            fontsTruncated = true;
                            truncated = true;
                            return nullptr;
                        }
                    }
                }
            }
            gf = made.get();
            fonts.push_back(std::move(made));
            byRef.emplace(std::make_pair(ref.num, ref.gen), gf);
        } else {
            gf = it->second;
        }
        if (gf->pageSeen != pageNum) {
            gf->pageSeen = pageNum;
            gf->pageMisses = 0;
        }
        if (pageFontIds.emplace(gf->id).second) {
            if (pageFonts.size() >= maxPageFonts) {
                fontsTruncated = true;
                pageFontIds.erase(gf->id);
                return nullptr;
            }
            pageFonts.push_back(gf);
        }
        return pageFontIds.count(gf->id) ? gf : nullptr;
    }

    XRef *xref;
    int pageNum = 0;
    std::vector<std::unique_ptr<GlyphFont>> fonts;
    std::map<std::pair<int, int>, GlyphFont *> byRef;
    std::vector<GlyphFont *> pageFonts;
    std::set<int> pageFontIds;
    std::set<std::pair<int, CharCode>> pageCodes;
    bool fontsTruncated = false, truncated = false;
    size_t pageBudget = 0, docGlyphs = 0;
    bool metadataExhausted = false;
    size_t metadataBytes = 0, cmapBytes = 0, mappingsLeft = 65536, sourceEntries = 0;
};

GlyphContext *glyphContext = nullptr;

} // namespace

// -page-facts-fd: the sink, the descriptor and whether a write failed
static PageFacts *pageFacts = nullptr;
static bool pageFactsWriteFailed = false;

// -timeout: SIGALRM ends the run with exit code 6. A facts line being written gets 2 more
// seconds, so the facts stay whole lines (without a run line, so a reader sees an incomplete run).
static constexpr int exitTimeout = 6;
#ifndef _WIN32
static volatile sig_atomic_t writingFactsLine = 0;
static volatile sig_atomic_t timeoutPassed = 0;

static void timeoutExit()
{
    if (!quiet) {
        static const char msg[] = "pdftotext: timeout\n";
        ssize_t ignored = write(2, msg, sizeof(msg) - 1);
        (void)ignored;
    }
    _exit(exitTimeout);
}

static void onTimeout(int /*sig*/)
{
    if (writingFactsLine && !timeoutPassed) {
        timeoutPassed = 1;
        alarm(2);
        return;
    }
    timeoutExit();
}
#endif

// A whole number of seconds in [0, 86400]: the whole string, digits only.
static bool parseSeconds(const char *text, size_t size, long long *seconds)
{
    const size_t len = strlen(text);
    if (len == 0 || len + 1 >= size) {
        return false;
    }
    long long v = 0;
    for (size_t i = 0; i < len; ++i) {
        if (text[i] < '0' || text[i] > '9' || v > 86400) {
            return false;
        }
        v = v * 10 + (text[i] - '0');
    }
    if (v > 86400) {
        return false;
    }
    *seconds = v;
    return true;
}

static void writeFactsLine(const std::string &line)
{
#ifndef _WIN32
    // A reader that closed the facts pipe must not kill the text pass with SIGPIPE: the facts
    // are a sidecar. SIGPIPE is blocked around the write, a pending one taken back, and EPIPE is
    // a failed facts write like any other (exit code 4 once the XHTML is complete).
    sigset_t pipeSet, oldSet;
    sigemptyset(&pipeSet);
    sigaddset(&pipeSet, SIGPIPE);
    const bool blocked = !pageFactsWriteFailed && pthread_sigmask(SIG_BLOCK, &pipeSet, &oldSet) == 0;
    // a timeout waits for the line (see onTimeout)
    writingFactsLine = 1;
    struct Done
    {
        ~Done()
        {
            writingFactsLine = 0;
            if (timeoutPassed) {
                timeoutExit();
            }
        }
    } done_;
    size_t done = 0;
    bool brokenPipe = false;
    while (!pageFactsWriteFailed && done < line.size()) {
        const ssize_t n = write(pageFactsFd, line.data() + done, line.size() - done);
        if (n < 0 && errno == EINTR) {
            continue;
        }
        if (n <= 0) {
            brokenPipe = n < 0 && errno == EPIPE;
            pageFactsWriteFailed = true;
            break;
        }
        done += static_cast<size_t>(n);
    }
    if (blocked) {
        if (brokenPipe) {
            // take back the SIGPIPE this write raised (when SIGPIPE was not blocked already, a
            // pending one can only be this write's)
            sigset_t pending;
            if (sigpending(&pending) == 0 && sigismember(&pending, SIGPIPE) && !sigismember(&oldSet, SIGPIPE)) {
                const struct timespec zero = { 0, 0 };
                sigtimedwait(&pipeSet, nullptr, &zero);
            }
        }
        pthread_sigmask(SIG_SETMASK, &oldSet, nullptr);
    }
#else
    (void)line;
    pageFactsWriteFailed = true;
#endif
}

static double millisecondsSince(std::chrono::steady_clock::time_point start)
{
    return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
}

static constexpr EndOfLineKind defaultEndOfLine()
{
#ifdef _WIN32
    return eolDOS;
#else
    return eolUnix;
#endif
}

int main(int argc, char *argv[])
{
    std::unique_ptr<PDFDoc> doc;
    std::unique_ptr<GooString> textFileName;
    std::optional<GooString> ownerPW, userPW;
    FILE *f;
    const UnicodeMap *uMap;
    Object info;
    bool ok;
    EndOfLineKind textEOL = defaultEndOfLine();

    Win32Console win32Console(&argc, &argv);

    // parse args
    ok = parseArgs(argDesc, &argc, argv);
    if (bboxLayout) {
        bbox = true;
    }
    if (bbox) {
        htmlMeta = true;
    }
    if (colspacing <= 0 || colspacing > 10) {
        error(errCommandLine, -1, "Bogus value provided for -colspacing");
        return 99;
    }
    if (collectRules && !bboxLayout) {
        error(errCommandLine, -1, "'-rules' needs -bbox-layout");
        return 99;
    }
    if (collectImages && !bboxLayout) {
        error(errCommandLine, -1, "'-images' needs -bbox-layout");
        return 99;
    }
    if (compactLayout && !bboxLayout) {
        error(errCommandLine, -1, "'-compact-layout' needs -bbox-layout");
        return 99;
    }
    if (glyphRecords && !(bboxLayout && wordMeta)) {
        error(errCommandLine, -1, "'-glyphs' needs -bbox-layout and -wordmeta");
        return 99;
    }
    if (wordMeta && !(bbox || tsvMode)) {
        error(errCommandLine, -1, "'-wordmeta' needs -bbox, -bbox-layout or -tsv");
        return 99;
    }
    unsigned factsFamilies = 0;
    if (pageFactsFd >= 0 || strcmp(pageFactsLevel, "basic") != 0 || pageFactsInclude[0] || pageFactsTiming) {
        if (pageFactsFd < 3 || !bboxLayout) {
            error(errCommandLine, -1, "'-page-facts-fd' needs -bbox-layout and a descriptor of 3 or more");
            return 99;
        }
        if (!PageFacts::parseLevel(pageFactsLevel, &factsFamilies) || !PageFacts::parseInclude(pageFactsInclude, &factsFamilies)) {
            error(errCommandLine, -1, "Bad '-page-facts-level' or '-page-facts-include' value");
            return 99;
        }
#ifndef _WIN32
        const int flags = fcntl(pageFactsFd, F_GETFL);
        if (flags < 0 || (flags & O_ACCMODE) == O_RDONLY) {
            error(errCommandLine, -1, "'-page-facts-fd': not a descriptor open for writing");
            return 99;
        }
#endif
    }
    long long timeoutBase = 0, timeoutPerPage = 0, timeoutMax = 0;
    if (!parseSeconds(timeoutArg, sizeof(timeoutArg), &timeoutBase) || !parseSeconds(timeoutPerPageArg, sizeof(timeoutPerPageArg), &timeoutPerPage) || !parseSeconds(timeoutMaxArg, sizeof(timeoutMaxArg), &timeoutMax)) {
        error(errCommandLine, -1, "Bad '-timeout', '-timeout-per-page' or '-timeout-max' value: whole seconds, at most 86400");
        return 99;
    }
    if (scanMaxWords < 0) {
        error(errCommandLine, -1, "Bad '-scan-max-words' value");
        return 99;
    }
    scanThresholds.maxVisibleWords = static_cast<size_t>(scanMaxWords);
    // a value that fills its buffer may have been cut short by parseArgs: refused
    if (strlen(scanMinCoverageArg) + 1 >= sizeof(scanMinCoverageArg) || !ScanThresholds::parseShare(scanMinCoverageArg, &scanThresholds.minCoverage)) {
        error(errCommandLine, -1, "Bad '-scan-min-coverage' value: a number in (0,1]");
        return 99;
    }
    if (strlen(scanLayerOverlapArg) + 1 >= sizeof(scanLayerOverlapArg) || !ScanThresholds::parseShare(scanLayerOverlapArg, &scanThresholds.layerOverlap)) {
        error(errCommandLine, -1, "Bad '-scan-layer-overlap' value: a number in (0,1]");
        return 99;
    }
    if (printURLs && (htmlMeta || tsvMode)) {
        error(errCommandLine, -1, "'-urls' is only supported in plain text mode");
        return 99;
    }
    if (!ok || (argc < 2 && !printEnc) || argc > 3 || printVersion || printHelp) {
        fprintf(stderr, "pdftotext version %s\n", PACKAGE_VERSION);
        fprintf(stderr, "%s\n", popplerCopyright);
        fprintf(stderr, "%s\n", xpdfCopyright);
        if (!printVersion) {
            printUsage("pdftotext", "<PDF-file> [<text-file>]", argDesc);
        }
        if (printVersion || printHelp) {
            return 0;
        }
        return 99;
    }

#ifndef _WIN32
    // the timeout runs from here: S (capped) until the page count is known
    const auto timeoutStart = std::chrono::steady_clock::now();
    const bool timeoutOn = timeoutBase > 0 || timeoutPerPage > 0;
    auto timeoutLimit = [&](long long pages) {
        long long limit = timeoutBase + std::min(timeoutPerPage * pages, 86400LL);
        if (timeoutMax > 0) {
            limit = std::min(limit, timeoutMax);
        }
        return std::max(limit, 1LL);
    };
    if (timeoutOn) {
        struct sigaction sa;
        memset(&sa, 0, sizeof(sa));
        sa.sa_handler = onTimeout;
        sigemptyset(&sa.sa_mask);
        sigaction(SIGALRM, &sa, nullptr);
        // before the document is open: S, or the cap when S is 0
        alarm(static_cast<unsigned>(timeoutBase > 0 ? timeoutLimit(0) : (timeoutMax > 0 ? timeoutMax : 86400)));
    }
#endif

    // read config file
    globalParams = std::make_unique<GlobalParams>();

    if (printEnc) {
        printEncodings();
        return 0;
    }

    GooString fileName(argv[1]);
    if (fixedPitch) {
        physLayout = true;
    }

    if (textEncName[0]) {
        globalParams->setTextEncoding(textEncName);
    }
    if (textEOLStr[0]) {
        if (!strcmp(textEOLStr, "unix")) {
            textEOL = eolUnix;
        } else if (!strcmp(textEOLStr, "dos")) {
            textEOL = eolDOS;
        } else if (!strcmp(textEOLStr, "mac")) {
            textEOL = eolMac;
        } else {
            fprintf(stderr, "Bad '-eol' value on command line\n");
        }
    }
    if (quiet) {
        globalParams->setErrQuiet(quiet);
    }
    std::optional<PageFacts> facts;
    const auto runStart = std::chrono::steady_clock::now();
    if (pageFactsFd >= 3) {
        facts.emplace(factsFamilies);
        facts->setScanThresholds(scanThresholds);
        facts->setTiming(pageFactsTiming);
        pageFacts = &*facts;
        // poppler's errors are also counted for the page, from their category and format string
        // (never formatted for it); they are printed as without facts unless -q
        setErrorFormatCallback(PageFacts::noteError);
    }

    EndOfLineHyphenMode hyphenMode = EndOfLineHyphenMode::RemoveAll;
    if (hyphenModeStr[0]) {
        using namespace std::string_view_literals;
        if (hyphenModeStr == "none"sv) {
            hyphenMode = EndOfLineHyphenMode::Keep;
        } else if (hyphenModeStr == "soft"sv) {
            hyphenMode = EndOfLineHyphenMode::RemoveSoft;
        } else if (hyphenModeStr == "all"sv) {
            hyphenMode = EndOfLineHyphenMode::RemoveAll;
        } else {
            error(errCommandLine, -1, "Invalid '-remove-hyphens' value");
            return 99;
        }
    }

    // get mapping to output encoding
    if (!(uMap = globalParams->getTextEncoding())) {
        error(errCommandLine, -1, "Couldn't get text encoding");
        return 99;
    }

    // open PDF file
    if (ownerPassword[0] != '\001') {
        ownerPW = GooString(ownerPassword);
    }
    if (userPassword[0] != '\001') {
        userPW = GooString(userPassword);
    }

    if (fileName.toStr() == "-") {
        fileName = GooString("fd://0");
    }

    doc = PDFDocFactory().createPDFDoc(fileName, ownerPW, userPW);

    if (!doc->isOk()) {
        if (pageFacts) {
            writeFactsLine(pageFacts->failedDocLine(doc->getErrorCode()));
            writeFactsLine(pageFacts->runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), nullptr));
        }
        return 1;
    }
    // pages usually share their fonts: parse each once per document (see GfxFontCache)
    doc->getXRef()->setFontCache(std::make_shared<GfxFontCache>());

#ifdef ENFORCE_PERMISSIONS
    // check for copy permission
    if (!doc->okToCopy()) {
        error(errNotAllowed, -1, "Copying of text from this document is not allowed.");
        return 3;
    }
#endif

    // construct text file name
    if (argc == 3) {
        textFileName = std::make_unique<GooString>(argv[2]);
    } else if (fileName.toStr() == "fd://0") {
        error(errCommandLine, -1, "You have to provide an output filename when reading from stdin.");
        return 99;
    } else {
        const char *p = fileName.c_str() + fileName.size() - 4;
        if (!strcmp(p, ".pdf") || !strcmp(p, ".PDF")) {
            textFileName = std::make_unique<GooString>(fileName.c_str(), fileName.size() - 4);
        } else {
            textFileName = fileName.copy();
        }
        textFileName->append(htmlMeta ? ".html" : ".txt");
    }

    // get page range
    if (firstPage < 1) {
        firstPage = 1;
    }
    if (lastPage < 1 || lastPage > doc->getNumPages()) {
        lastPage = doc->getNumPages();
    }
    if (lastPage < firstPage) {
        error(errCommandLine, -1, "Wrong page range given: the first page ({0:d}) can not be after the last page ({1:d}).", firstPage, lastPage);
        return 99;
    }
#ifndef _WIN32
    if (timeoutOn) {
        // the limit for the range, less the time spent opening the document
        const auto spent = std::chrono::duration_cast<std::chrono::seconds>(std::chrono::steady_clock::now() - timeoutStart).count();
        const long long left = timeoutLimit(lastPage - firstPage + 1) - static_cast<long long>(spent);
        if (left <= 0) {
            timeoutExit();
        }
        alarm(static_cast<unsigned>(left));
    }
#endif
    if (pageFacts) {
        writeFactsLine(pageFacts->docLine(doc.get(), ownerPW.has_value() || userPW.has_value()));
    }
    // This process consumes the selected range once and never asks for an earlier page. Avoid
    // constructing and retaining every Page before a late -f; the page-tree walk and its error
    // checks remain unchanged.
    doc->skipPageCacheBefore(firstPage);

    // write HTML header
    if (htmlMeta) {
        if (textFileName->toStr() == "-") {
            f = stdout;
        } else {
            if (!(f = fopen(textFileName->c_str(), "wb"))) {
                error(errIO, -1, "Couldn't open text file '{0:t}'", textFileName.get());
                return 2;
            }
        }
        fputs(R"(<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">)", f);
        fputs("<html xmlns=\"http://www.w3.org/1999/xhtml\">\n", f);
        fputs("<head>\n", f);
        info = doc->getDocInfo();
        if (info.isDict()) {
            Object obj = info.getDict()->lookup("Title");
            if (obj.isString()) {
                printInfoString(f, info.getDict(), "Title", "<title>", "</title>\n", uMap);
            } else {
                fputs("<title></title>\n", f);
            }
            printInfoString(f, info.getDict(), "Subject", R"(<meta name="Subject" content=")", "\"/>\n", uMap);
            printInfoString(f, info.getDict(), "Keywords", R"(<meta name="Keywords" content=")", "\"/>\n", uMap);
            printInfoString(f, info.getDict(), "Author", R"(<meta name="Author" content=")", "\"/>\n", uMap);
            printInfoString(f, info.getDict(), "Creator", R"(<meta name="Creator" content=")", "\"/>\n", uMap);
            printInfoString(f, info.getDict(), "Producer", R"(<meta name="Producer" content=")", "\"/>\n", uMap);
            printInfoDate(f, info.getDict(), "CreationDate", R"(<meta name="CreationDate" content=")", "\"/>\n");
            printInfoDate(f, info.getDict(), "ModDate", R"(<meta name="ModDate" content=")", "\"/>\n");
        }
        fputs("</head>\n", f);
        fputs("<body>\n", f);
        if (!bbox) {
            fputs("<pre>\n", f);
            if (f != stdout) {
                fclose(f);
            }
        }
    }

    // write text file
    if (htmlMeta && bbox) { // htmlMeta && is superfluous but makes gcc happier
        TextOutputDev textOut(nullptr, physLayout, fixedPitch, rawOrder, htmlMeta, discardDiag);

        if (textOut.isOk()) {
            textOut.setTextEOL(textEOL);
            textOut.setMinColSpacing1(colspacing);
            textOut.setEndOfLineHyphenMode(hyphenMode);
            if (noPageBreaks) {
                textOut.setTextPageBreaks(false);
            }
            if (bboxLayout) {
                textOut.setPageFacts(pageFacts);
                textOut.setCollectRules(collectRules);
                textOut.setCollectImages(collectImages);
                textOut.setCollectImageMaskRules(glyphRecords);
                textOut.setCollectDroppedGlyphs(glyphRecords);
                printDocBBox(f, doc.get(), &textOut, firstPage, lastPage);
            } else {
                printWordBBox(f, doc.get(), &textOut, firstPage, lastPage);
            }
        }
        if (f != stdout) {
            fclose(f);
        }
    } else {

        if (tsvMode) {
            TextOutputDev textOut(nullptr, physLayout, fixedPitch, rawOrder, htmlMeta, discardDiag);
            textOut.setEndOfLineHyphenMode(hyphenMode);
            if (textFileName->toStr() == "-") {
                f = stdout;
            } else {
                if (!(f = fopen(textFileName->c_str(), "wb"))) {
                    error(errIO, -1, "Couldn't open text file '{0:t}'", textFileName.get());
                    return 2;
                }
            }
            printTSVBBox(f, doc.get(), &textOut, firstPage, lastPage);
            if (f != stdout) {
                fclose(f);
            }
        } else {
            TextOutputDev textOut(textFileName->c_str(), physLayout, fixedPitch, rawOrder, htmlMeta, discardDiag);
            if (textOut.isOk()) {
                textOut.setTextEOL(textEOL);
                textOut.setMinColSpacing1(colspacing);
                textOut.setEndOfLineHyphenMode(hyphenMode);
                textOut.setInlineLinkURIs(printURLs);
                if (noPageBreaks) {
                    textOut.setTextPageBreaks(false);
                }

                if ((w == 0) && (h == 0) && (x == 0) && (y == 0)) {
                    doc->displayPages(&textOut, firstPage, lastPage, resolution, resolution, 0, true, false, false);
                } else {

                    for (int page = firstPage; page <= lastPage; ++page) {
                        doc->displayPageSlice(&textOut, page, resolution, resolution, 0, true, false, false, x, y, w, h);
                    }
                }

            } else {
                return 2;
            }
        }
    }

    if (pageFacts) {
        writeFactsLine(pageFacts->runLine(PACKAGE_VERSION, DOPPLER_COMMIT, millisecondsSince(runStart), nullptr));
    }

    // write end of HTML file
    if (htmlMeta) {
        if (textFileName->toStr() == "-") {
            f = stdout;
        } else {
            if (!(f = fopen(textFileName->c_str(), "ab"))) {
                error(errIO, -1, "Couldn't open text file '{0:t}'", textFileName.get());
                return 2;
            }
        }
        if (!bbox) {
            fputs("</pre>\n", f);
        }
        fputs("</body>\n", f);
        fputs("</html>\n", f);
        if (f != stdout) {
            fclose(f);
        }
    }

    if (pageFactsWriteFailed) {
        // the text output is complete: its own exit code, so a caller can keep it
        error(errIO, -1, "Couldn't write the page facts");
        return 4;
    }
    return 0;
}

static void printInfoString(FILE *f, Dict *infoDict, const char *key, const char *text1, const char *text2, const UnicodeMap *uMap)
{
    bool isUnicode;
    Unicode u;
    char buf[9];
    size_t i, n;

    Object obj = infoDict->lookup(key);
    if (obj.isString()) {
        fputs(text1, f);
        const std::string &s1 = obj.getString();
        if (s1.length() > 1 && (s1.at(0) & 0xff) == 0xfe && (s1.at(1) & 0xff) == 0xff) {
            isUnicode = true;
            i = 2;
        } else {
            isUnicode = false;
            i = 0;
        }
        while (i < obj.getString().size()) {
            if (isUnicode) {
                if (i + 1 >= s1.size()) {
                    break; // an odd byte count: the last byte is half a code unit
                }
                u = ((s1.at(i) & 0xff) << 8) | (s1.at(i + 1) & 0xff);
                i += 2;
            } else {
                u = pdfDocEncoding[s1.at(i) & 0xff];
                ++i;
            }
            n = uMap->mapUnicode(u, buf, sizeof(buf));
            buf[n] = '\0';
            const std::string myString = myXmlTokenReplace(buf);
            fputs(myString.c_str(), f);
        }
        fputs(text2, f);
    }
}

static void printInfoDate(FILE *f, Dict *infoDict, const char *key, const char *text1, const char *text2)
{
    int year, mon, day, hour, min, sec, tz_hour, tz_minute;
    char tz;

    Object obj = infoDict->lookup(key);
    if (obj.isString()) {
        const std::string &s = obj.getString();
        if (parseDateString(s, &year, &mon, &day, &hour, &min, &sec, &tz, &tz_hour, &tz_minute)) {
            fputs(text1, f);
            fprintf(f, "%04d-%02d-%02dT%02d:%02d:%02d", year, mon, day, hour, min, sec);
            if (tz_hour == 0 && tz_minute == 0) {
                fprintf(f, "Z");
            } else {
                fprintf(f, "%c%02d", tz, tz_hour);
                if (tz_minute) {
                    fprintf(f, ":%02d", tz_minute);
                }
            }
            fputs(text2, f);
        }
    }
}

static void printLine(FILE *f, const TextLine *line, const UnicodeMap *uMap)
{
    if (compactLayout) {
        // The converter needs geometric relationships, not six decimal places.
        // Avoid a locale-aware stream insertion for every coordinate. This buffer
        // also fits the fixed representation of the largest finite double.
        std::string words;
        words.reserve(4096);
        std::optional<WordMetadata> previousMetadata;
        std::string previousAttributes;
        const auto coordinate = [&](double value) {
            char buffer[384];
            const auto result = std::to_chars(buffer, buffer + sizeof(buffer), value, std::chars_format::fixed, 3);
            if (result.ec == std::errc()) {
                words.append(buffer, result.ptr);
            } else {
                words += std::to_string(value);
            }
        };
        const TextBounds bounds = projectLine(line, uMap, wordMeta, [&](const ProjectedWord &word) {
            words += "          <word xMin=\"";
            coordinate(word.bounds.xMin);
            words += "\" yMin=\"";
            coordinate(word.bounds.yMin);
            words += "\" xMax=\"";
            coordinate(word.bounds.xMax);
            words += "\" yMax=\"";
            coordinate(word.bounds.yMax);
            words += "\"";
            if (word.metadata) {
                if (!previousMetadata || *previousMetadata != *word.metadata) {
                    previousAttributes = wordMetaXmlAttributes(*word.metadata);
                    previousMetadata = *word.metadata;
                }
                words += previousAttributes;
            }
            words += ">";
            words += myXmlTokenReplace(word.text->c_str());
            words += "</word>\n";
            if (glyphContext) {
                const size_t before = words.size();
                glyphContext->appendWordGlyphs(words, word.word);
                if (words.size() != before) {
                    words += '\n';
                }
            }
        });
        putBoxTag(f, "        <line", bounds.xMin, bounds.yMin, bounds.xMax, bounds.yMax);
        fwrite(words.data(), 1, words.size(), f);
        fputs("        </line>\n", f);
        return;
    }
    std::stringstream wordXML;
    wordXML << std::fixed << std::setprecision(6);

    const TextBounds bounds = projectLine(line, uMap, wordMeta, [&](const ProjectedWord &word) {
        const TextBounds &b = word.bounds;
        wordXML << "          <word xMin=\"" << b.xMin << "\" yMin=\"" << b.yMin << "\" xMax=\"" << b.xMax << "\" yMax=\"" << b.yMax << "\"" << (word.metadata ? wordMetaXmlAttributes(*word.metadata) : std::string()) << ">"
                << myXmlTokenReplace(word.text->c_str()) << "</word>\n";
        if (glyphContext) {
            std::string glyphs;
            glyphContext->appendWordGlyphs(glyphs, word.word);
            if (!glyphs.empty()) {
                wordXML << glyphs << "\n";
            }
        }
    });
    fprintf(f, "        <line xMin=\"%f\" yMin=\"%f\" xMax=\"%f\" yMax=\"%f\">\n", bounds.xMin, bounds.yMin, bounds.xMax, bounds.yMax);
    fputs(wordXML.str().c_str(), f);
    fputs("        </line>\n", f);
}

void printDocBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last)
{
    double xMin, yMin, xMax, yMax;
    const TextFlow *flow;
    const TextBlock *blk;
    const TextLine *line;
    // look up the output encoding once, not for every word
    const UnicodeMap *uMap = globalParams->getTextEncoding();

    std::optional<GlyphContext> glyphs;
    if (glyphRecords) {
        glyphs.emplace(doc->getXRef());
    }
    glyphContext = glyphs ? &*glyphs : nullptr;
    fprintf(f, "<doc>\n");
    for (int page = first; page <= last; ++page) {
        const auto pageStart = std::chrono::steady_clock::now();
        if (pageFacts) {
            pageFacts->beginPage(page);
        }
        doc->displayPage(textOut, page, resolution, resolution, 0, !useCropBox, useCropBox, false);
        if (pageFacts) {
            writeFactsLine(pageFacts->finishPage(page, millisecondsSince(pageStart)));
        }
        // The page size must be in the same space as the word coordinates (rotated and scaled)
        const double wid = textOut->getPageWidth();
        const double hgt = textOut->getPageHeight();
        std::string glyphAttrs;
        if (glyphContext) {
            glyphContext->startPage(page, textOut);
            glyphAttrs = std::string(glyphContext->pageTruncated() ? " glyphsTruncated=\"1\"" : "") + (glyphContext->pageFontsTruncated() ? " fontsTruncated=\"1\"" : "");
        }
        std::vector<TextGraphic> graphics;
        if (collectImages) {
            graphics = textOut->takeGraphics();
            // after the glyph attributes, so a reader that stops at the first unknown one still sees them
            glyphAttrs += " number=\"" + std::to_string(page) + "\"" + (textOut->getImagesTruncated() ? " imagesTruncated=\"1\"" : "") + (textOut->getGraphicsTruncated() ? " graphicsTruncated=\"1\"" : "");
        }
        if (cropSizes) {
            if (!collectImages) {
                // the PDF's page number, so page sizes of a -f/-l range name their pages
                glyphAttrs += " number=\"" + std::to_string(page) + "\"";
            }
            // last, so a reader that stops at the first unknown attribute still sees the others; %g as pdfinfo
            char crop[64];
            snprintf(crop, sizeof(crop), " crop=\"%g %g\"", doc->getPageCropWidth(page), doc->getPageCropHeight(page));
            glyphAttrs += crop;
        }
        ScanVerdict scan;
        // a cross-reference table rebuilt from a scan of the file (at open, or by a fetch that
        // met a broken entry, on this page or before): a reference may now resolve to another
        // object body than a reader that never rebuilt it sees, so no image of this page or a
        // later one is copyable, and the page says so (xrefRebuilt="1", which a reader keeps
        // for the rest of the document). With -rules too: the rules were drawn from objects that
        // may differ from what a run that never rebuilt the table reads
        const bool xrefRebuilt = doc->getXRef()->getReconstructionCount() > 0;
        if (collectImages) {
            // last: a full-page scan (see classifyScan) and the image that covers it
            scan = textOut->getScan(scanThresholds);
            if (scan.kind != ScanVerdict::None) {
                glyphAttrs += std::string(" scan=\"") + scanKindName(scan.kind) + "\" scanImage=\"" + std::to_string(scan.image) + "\"";
            }
        }
        if ((collectImages || collectRules) && xrefRebuilt) {
            glyphAttrs += " xrefRebuilt=\"1\"";
        }
        if (collectRules) {
            fprintf(f, "  <page width=\"%f\" height=\"%f\"%s%s>\n", wid, hgt, textOut->getRulesTruncated() ? " rulesTruncated=\"1\"" : "", glyphAttrs.c_str());
            for (const TextRule &rule : textOut->getRules()) {
                if (rule.pastDocumentCap) {
                    break; // kept for page facts only; they come last
                }
                fprintf(f, "    <%s xMin=\"%.2f\" yMin=\"%.2f\" xMax=\"%.2f\" yMax=\"%.2f\" color=\"%s\"%s/>\n", rule.box ? "box" : "rule", rule.xMin, rule.yMin, rule.xMax, rule.yMax, hexColor(rule.r, rule.g, rule.b).c_str(),
                        rule.imageMask ? " src=\"mask\"" : "");
            }
        } else {
            fprintf(f, "  <page width=\"%f\" height=\"%f\"%s>\n", wid, hgt, glyphAttrs.c_str());
        }
        if (collectImages) {
            int imageNumber = 0;
            for (const TextImage &image : textOut->getImages()) {
                ++imageNumber;
                if (image.pastDocumentCap) {
                    break; // kept for page facts and scans only; they come last
                }
                // ref: the image XObject, so repeats of one object can be found (none for inline images)
                const std::string ref = image.ref == Ref::INVALID() ? std::string() : " ref=\"" + std::to_string(image.ref.num) + " " + std::to_string(image.ref.gen) + "\"";
                // the dictionary's facts, last: codec, bits, colour space, masks, and how the pixels
                // can be taken out (copy class)
                const ImageDictInfo &info = image.info;
                std::string dict = std::string(" filter=\"") + imageFilterName(info.filter) + "\" bpc=\"" + std::to_string(info.bpc) + "\" cs=\"" + imageColorName(info.color) + "\"";
                if (info.softMask || info.mask) {
                    dict += " smask=\"1\"";
                }
                if (info.decode) {
                    dict += " decode=\"1\"";
                }
                if (info.inlineImg) {
                    dict += " inline=\"1\"";
                }
                dict += std::string(" copy=\"") + imageCopyName(info.copy) + "\"";
                // last: how its pixels land on the page (a copy of them shows the page only when
                // upright), and whether anything else shows over them (TextImage::overlaid);
                // always written, so a reader can treat their absence as "do not copy, do not
                // link as a repeat"
                static const char *const orientations[] = { "0", "90", "180", "270", "other" };
                dict += std::string(" orient=\"") + orientations[image.orientation] + "\"";
                dict += image.overlaid ? " overlaid=\"1\"" : " overlaid=\"0\"";
                // last: a single scan whose page passed the copy allow-list (ScanCopyAudit:
                // nothing but this one image paints on the page); absent means render it
                if (!xrefRebuilt && scan.kind == ScanVerdict::Single && scan.image == imageNumber && image.ref != Ref::INVALID() && textOut->getPageScanAudit().copyable(image.ref)) {
                    dict += " copyable=\"1\"";
                }
                fprintf(f, "    <image xMin=\"%.2f\" yMin=\"%.2f\" xMax=\"%.2f\" yMax=\"%.2f\" width=\"%d\" height=\"%d\"%s%s%s/>\n", image.xMin, image.yMin, image.xMax, image.yMax, image.pixelWidth, image.pixelHeight,
                        image.stencil ? " kind=\"mask\"" : "", ref.c_str(), dict.c_str());
            }
            for (const TextGraphic &g : graphics) {
                fprintf(f, "    <graphic xMin=\"%.2f\" yMin=\"%.2f\" xMax=\"%.2f\" yMax=\"%.2f\" paths=\"%zu\" segments=\"%zu\" curves=\"%zu\" directions=\"%d\"/>\n", g.xMin, g.yMin, g.xMax, g.yMax, g.paths, g.segments, g.curves,
                        std::popcount(g.directions));
            }
        }
        if (glyphContext) {
            glyphContext->writeFontTable(f);
            glyphContext->writeDropped(f, textOut);
        }
        for (flow = textOut->getFlows(); flow; flow = flow->getNext()) {
            fprintf(f, "    <flow>\n");
            for (blk = flow->getBlocks(); blk; blk = blk->getNext()) {
                blk->getBBox(&xMin, &yMin, &xMax, &yMax);
                putBoxTag(f, "      <block", xMin, yMin, xMax, yMax);
                for (line = blk->getLines(); line; line = line->getNext()) {
                    printLine(f, line, uMap);
                }
                fprintf(f, "      </block>\n");
            }
            fprintf(f, "    </flow>\n");
        }
        fprintf(f, "  </page>\n");
    }
    // the table rebuilt after the last page's attributes were written (or at all): a reader
    // that has seen copyable="1" on an earlier page must not copy it
    if (collectImages && doc->getXRef()->getReconstructionCount() > 0) {
        fprintf(f, "  <xref rebuilt=\"1\"/>\n");
    }
    fprintf(f, "</doc>\n");
    glyphContext = nullptr;
}

void printTSVBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last)
{
    const TextFlow *flow;
    const TextBlock *blk;
    const TextLine *line;
    const int pageLevel = 1;
    const int blockLevel = 3;
    const int lineLevel = 4;
    const int wordLevel = 5;
    const int metaConf = -1;
    const int wordConf = 100;
    // look up the output encoding once, not for every word
    const UnicodeMap *uMap = globalParams->getTextEncoding();

    // with -wordmeta every row has the extra columns (empty for page, block and line rows)
    const char *const metaEmpty = wordMeta ? tsvWordMetaEmpty : "";
    fprintf(f, "level\tpage_num\tpar_num\tblock_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext%s\n", wordMeta ? tsvWordMetaHeader : "");

    for (int page = first; page <= last; ++page) {
        doc->displayPage(textOut, page, resolution, resolution, 0, !useCropBox, useCropBox, false);
        // The page size must be in the same space as the word coordinates (rotated and scaled)
        const double wid = textOut->getPageWidth();
        const double hgt = textOut->getPageHeight();

        fprintf(f, "%d\t%d\t%d\t%d\t%d\t%d\t%f\t%f\t%f\t%f\t%d\t###PAGE###%s\n", pageLevel, page, 0, 0, 0, 0, 0.0, 0.0, wid, hgt, metaConf, metaEmpty);

        double xMin = 0, yMin = 0, xMax = 0, yMax = 0;
        int flowNum = 0;

        for (flow = textOut->getFlows(); flow; flow = flow->getNext()) {
            // flow->getBBox(&xMin, &yMin, &xMax, &yMax);
            // fprintf(f, "%d\t%d\t%d\t%d\t%d\t%f\t%f\t%f\t%f\t\n", page,flowNum,blockNum,lineNum,wordNum,xMin,yMin,wid, hgt);

            int blockNum = 0;

            for (blk = flow->getBlocks(); blk; blk = blk->getNext()) {
                int lineNum = 0;

                blk->getBBox(&xMin, &yMin, &xMax, &yMax);
                fprintf(f, "%d\t%d\t%d\t%d\t%d\t%d\t%f\t%f\t%f\t%f\t%d\t###FLOW###%s\n", blockLevel, page, flowNum, blockNum, lineNum, 0, xMin, yMin, xMax - xMin, yMax - yMin, metaConf, metaEmpty);

                for (line = blk->getLines(); line; line = line->getNext()) {
                    int wordNum = 0;

                    std::string lineWordsBuffer;

                    const TextBounds bounds = projectLine(line, uMap, wordMeta, [&](const ProjectedWord &word) {
                        const TextBounds &b = word.bounds;
                        GooString::appendf(lineWordsBuffer, "{0:d}\t{1:d}\t{2:d}\t{3:d}\t{4:d}\t{5:d}\t{6:.2f}\t{7:.2f}\t{8:.2f}\t{9:.2f}\t{10:d}\t{11:s}", wordLevel, page, flowNum, blockNum, lineNum, wordNum, b.xMin, b.yMin,
                                           b.xMax - b.xMin, b.yMax - b.yMin, wordConf, myTsvFieldReplace(word.text->c_str()).c_str());
                        if (word.metadata) {
                            lineWordsBuffer += wordMetaTsvColumns(*word.metadata);
                        }
                        lineWordsBuffer += '\n';
                        wordNum++;
                    });

                    fprintf(f, "%d\t%d\t%d\t%d\t%d\t%d\t%f\t%f\t%f\t%f\t%d\t###LINE###%s\n", lineLevel, page, flowNum, blockNum, lineNum, 0, bounds.xMin, bounds.yMin, bounds.xMax - bounds.xMin, bounds.yMax - bounds.yMin, metaConf,
                            metaEmpty);
                    fprintf(f, "%s", lineWordsBuffer.c_str());
                    lineNum++;
                }
                blockNum++;
            }
            flowNum++;
        }
    }
}

void printWordBBox(FILE *f, PDFDoc *doc, TextOutputDev *textOut, int first, int last)
{
    // look up the output encoding once, not for every word
    const UnicodeMap *uMap = globalParams->getTextEncoding();
    fprintf(f, "<doc>\n");
    for (int page = first; page <= last; ++page) {
        doc->displayPage(textOut, page, resolution, resolution, 0, !useCropBox, useCropBox, false);
        // The page size must be in the same space as the word coordinates (rotated and scaled)
        const double wid = textOut->getPageWidth();
        const double hgt = textOut->getPageHeight();
        fprintf(f, "  <page width=\"%f\" height=\"%f\">\n", wid, hgt);
        const std::unique_ptr<TextWordList> wordlist = textOut->makeWordList();
        const std::vector<TextWord *> &words = wordlist->getWords();

        if (words.empty()) {
            fprintf(stderr, "no word list\n");
        } else {
            for (const TextWord *word : words) {
                const ProjectedWord projected = projectWord(word, uMap, wordMeta);
                const TextBounds &b = projected.bounds;
                const std::string myString = myXmlTokenReplace(projected.text->c_str());
                fprintf(f, "    <word xMin=\"%f\" yMin=\"%f\" xMax=\"%f\" yMax=\"%f\"%s>%s</word>\n", b.xMin, b.yMin, b.xMax, b.yMax, projected.metadata ? wordMetaXmlAttributes(*projected.metadata).c_str() : "", myString.c_str());
            }
        }
        fprintf(f, "  </page>\n");
    }
    fprintf(f, "</doc>\n");
}
