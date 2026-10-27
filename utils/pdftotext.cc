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
#include <cstdio>
#include <cstdlib>
#include <cstddef>
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
static bool compactLayout = false;
static char hyphenModeStr[16] = "";

static const ArgDesc argDesc[] = { { .arg = "-f", .kind = argInt, .val = &firstPage, .size = 0, .usage = "first page to convert" },
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
                                   { .arg = "-compact-layout", .kind = argFlag, .val = &compactLayout, .size = 0, .usage = "with -bbox-layout: fast locale-independent word coordinates at 0.001-point precision" },
                                   { .arg = "-enc", .kind = argString, .val = textEncName, .size = sizeof(textEncName), .usage = "output text encoding name" },
                                   { .arg = "-listenc", .kind = argFlag, .val = &printEnc, .size = 0, .usage = "list available encodings" },
                                   { .arg = "-eol", .kind = argString, .val = textEOLStr, .size = sizeof(textEOLStr), .usage = "output end-of-line convention (unix, dos, or mac)" },
                                   { .arg = "-nopgbrk", .kind = argFlag, .val = &noPageBreaks, .size = 0, .usage = "don't insert page breaks between pages" },
                                   { .arg = "-urls", .kind = argFlag, .val = &printURLs, .size = 0, .usage = "print the URL of each link next to its link text" },
                                   { .arg = "-bbox", .kind = argFlag, .val = &bbox, .size = 0, .usage = "output bounding box for each word and page size to html. Sets -htmlmeta" },
                                   { .arg = "-rules", .kind = argFlag, .val = &collectRules, .size = 0, .usage = "with -bbox-layout: also output axis-aligned vector rules and filled boxes (table borders, shading)" },
                                   { .arg = "-bbox-layout", .kind = argFlag, .val = &bboxLayout, .size = 0, .usage = "like -bbox but with extra layout bounding box data.  Sets -htmlmeta" },
                                   { .arg = "-cropbox", .kind = argFlag, .val = &useCropBox, .size = 0, .usage = "use the crop box rather than media box" },
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
                                   {} };

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

// -wordmeta: extra attributes of a <word> element, each preceded by a space.
static std::string wordMetaXmlAttributes(const WordMetadata &meta)
{
    char buf[256];
    snprintf(buf, sizeof(buf), " fontSize=\"%f\" rotation=\"%d\" spaceAfter=\"%d\" invisible=\"%d\" color=\"%s\" baseline=\"%f\"", meta.fontSize, meta.rotation, meta.spaceAfter ? 1 : 0, meta.invisible ? 1 : 0,
             hexColor(meta.r, meta.g, meta.b).c_str(), meta.baseline);
    return " fontName=\"" + myXmlTokenReplace(meta.fontName.c_str()) + "\"" + buf;
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
    if (compactLayout && !bboxLayout) {
        error(errCommandLine, -1, "'-compact-layout' needs -bbox-layout");
        return 99;
    }
    if (wordMeta && !(bbox || tsvMode)) {
        error(errCommandLine, -1, "'-wordmeta' needs -bbox, -bbox-layout or -tsv");
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
        return 1;
    }

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
                textOut.setCollectRules(collectRules);
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
        });
        fprintf(f, "        <line xMin=\"%f\" yMin=\"%f\" xMax=\"%f\" yMax=\"%f\">\n", bounds.xMin, bounds.yMin, bounds.xMax, bounds.yMax);
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

    fprintf(f, "<doc>\n");
    for (int page = first; page <= last; ++page) {
        doc->displayPage(textOut, page, resolution, resolution, 0, !useCropBox, useCropBox, false);
        // The page size must be in the same space as the word coordinates (rotated and scaled)
        const double wid = textOut->getPageWidth();
        const double hgt = textOut->getPageHeight();
        if (collectRules) {
            fprintf(f, "  <page width=\"%f\" height=\"%f\"%s>\n", wid, hgt, textOut->getRulesTruncated() ? " rulesTruncated=\"1\"" : "");
            for (const TextRule &rule : textOut->getRules()) {
                fprintf(f, "    <%s xMin=\"%.2f\" yMin=\"%.2f\" xMax=\"%.2f\" yMax=\"%.2f\" color=\"%s\"/>\n", rule.box ? "box" : "rule", rule.xMin, rule.yMin, rule.xMax, rule.yMax, hexColor(rule.r, rule.g, rule.b).c_str());
            }
        } else {
            fprintf(f, "  <page width=\"%f\" height=\"%f\">\n", wid, hgt);
        }
        for (flow = textOut->getFlows(); flow; flow = flow->getNext()) {
            fprintf(f, "    <flow>\n");
            for (blk = flow->getBlocks(); blk; blk = blk->getNext()) {
                blk->getBBox(&xMin, &yMin, &xMax, &yMax);
                fprintf(f, "      <block xMin=\"%f\" yMin=\"%f\" xMax=\"%f\" yMax=\"%f\">\n", xMin, yMin, xMax, yMax);
                for (line = blk->getLines(); line; line = line->getNext()) {
                    printLine(f, line, uMap);
                }
                fprintf(f, "      </block>\n");
            }
            fprintf(f, "    </flow>\n");
        }
        fprintf(f, "  </page>\n");
    }
    fprintf(f, "</doc>\n");
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
