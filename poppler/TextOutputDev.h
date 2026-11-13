//========================================================================
//
// TextOutputDev.h
//
// Copyright 1997-2003 Glyph & Cog, LLC
//
//========================================================================

//========================================================================
//
// Modified under the Poppler project - http://poppler.freedesktop.org
//
// All changes made under the Poppler project to this file are licensed
// under GPL version 2 or later
//
// Copyright (C) 2005-2007 Kristian Høgsberg <krh@redhat.com>
// Copyright (C) 2006 Ed Catmur <ed@catmur.co.uk>
// Copyright (C) 2007, 2008, 2011, 2013 Carlos Garcia Campos <carlosgc@gnome.org>
// Copyright (C) 2007, 2017 Adrian Johnson <ajohnson@redneon.com>
// Copyright (C) 2008, 2010, 2015, 2016, 2018, 2019, 2021, 2025, 2026 Albert Astals Cid <aacid@kde.org>
// Copyright (C) 2010 Brian Ewins <brian.ewins@gmail.com>
// Copyright (C) 2012, 2013, 2015, 2016 Jason Crain <jason@aquaticape.us>
// Copyright (C) 2013 Thomas Freitag <Thomas.Freitag@alfa.de>
// Copyright (C) 2018 Klarälvdalens Datakonsult AB, a KDAB Group company, <info@kdab.com>. Work sponsored by the LiMux project of the city of Munich
// Copyright (C) 2018 Sanchit Anand <sanxchit@gmail.com>
// Copyright (C) 2018, 2020, 2021, 2025 Nelson Benítez León <nbenitezl@gmail.com>
// Copyright (C) 2019, 2022 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright (C) 2019 Dan Shea <dan.shea@logical-innovations.com>
// Copyright (C) 2020 Suzuki Toshiya <mpsuzuki@hiroshima-u.ac.jp>
// Copyright (C) 2024, 2025 Stefan Brüns <stefan.bruens@rwth-aachen.de>
// Copyright (C) 2024-2026 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright (C) 2025 Hagen Möbius <hagen.moebius@googlemail.com>
// Copyright (C) 2026 Aditya Tiwari <suntiwari3495@gmail.com>
// Copyright (C) 2026 Diego Costa R. <diegocostar@gmail.com>
//
// To see a description of the changes please see the Changelog file that
// came with your tarball or type make ChangeLog if you are building from git
//
//========================================================================

#ifndef TEXTOUTPUTDEV_H
#define TEXTOUTPUTDEV_H

#include "poppler_private_export.h"
#include "GfxFont.h"
#include "GfxState.h"
#include "OutputDev.h"
#include "PDFRectangle.h"
#include "PageFacts.h"

#include <array>
#include <vector>

class GooString;
class Gfx;
class GfxFont;
class GfxState;
class UnicodeMap;
class AnnotLink;

class TextWord;
class TextPool;
class TextLine;
class TextLineFrag;
class TextBlock;
class TextFlow;
class TextLink;
class TextUnderline;
class TextWordList;
class TextPage;
class TextSelectionVisitor;

//------------------------------------------------------------------------

using TextOutputFunc = void (*)(void *stream, const char *text, int len);

enum SelectionStyle
{
    selectionStyleGlyph,
    selectionStyleWord,
    selectionStyleLine
};

enum EndOfLineKind
{
    eolUnix, // LF
    eolDOS, // CR+LF
    eolMac // CR
};

enum class EndOfLineHyphenMode
{
    RemoveAll,
    RemoveSoft,
    Keep
};

//------------------------------------------------------------------------
// TextFontInfo
//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT TextFontInfo
{
public:
    explicit TextFontInfo(const GfxState *state);
    ~TextFontInfo();

    TextFontInfo(const TextFontInfo &) = delete;
    TextFontInfo &operator=(const TextFontInfo &) = delete;

    bool matches(const GfxState *state) const;
    bool matches(const TextFontInfo *fontInfo) const;
    bool matches(const Ref *ref) const;

    // Get the font ascent, or a default value if the font is not set
    double getAscent() const;

    // Get the font descent, or a default value if the font is not set
    double getDescent() const;

    // Get the writing mode, or Horizontal if the font is not set
    GfxFont::WritingMode getWMode() const;

    // Get the font name (which may be NULL).
    const GooString *getFontName() const { return fontName; }

    // Get the font (which may be NULL).
    const GfxFont *getGfxFont() const { return gfxFont.get(); }

    // Get font descriptor flags.
    bool isFixedWidth() const { return flags & fontFixedWidth; }
    bool isSerif() const { return flags & fontSerif; }
    bool isSymbolic() const { return flags & fontSymbolic; }
    bool isItalic() const { return flags & fontItalic; }
    bool isBold() const { return flags & fontBold; }

private:
    std::shared_ptr<GfxFont> gfxFont;
    GooString *fontName;
    int flags;

    friend class TextWord;
    friend class TextPage;
    friend class TextSelectionPainter;
};

//------------------------------------------------------------------------
// TextWord
//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT TextWord
{
public:
    // Constructor.
    TextWord(const GfxState *state, int rotA, double fontSize);

    // Destructor.
    ~TextWord();

    TextWord(const TextWord &) = delete;
    TextWord &operator=(const TextWord &) = delete;

    // Add a character to the word.
    void addChar(TextFontInfo *fontA, double x, double y, double dx, double dy, int charPosA, int charLen, CharCode c, Unicode u, const Matrix &textMatA);

    // Attempt to add a character to the word as a combining character.
    // Either character u or the last character in the word must be an
    // acute, dieresis, or other combining character.  Returns true if
    // the character was added.
    bool addCombining(TextFontInfo *fontA, double fontSizeA, double x, double y, double dx, double dy, int charPosA, int charLen, CharCode c, Unicode u, const Matrix &textMatA);

    // Merge <word> onto the end of <this>.
    void merge(TextWord *word);

    // Compares <this> to <word>, returning -1 (<), 0 (=), or +1 (>),
    // based on a primary-axis comparison, e.g., x ordering if rot=0.
    int primaryCmp(const TextWord *word) const;

    // Return the distance along the primary axis between <this> and
    // <word>.
    double primaryDelta(const TextWord *word) const;

    static bool cmpYX(const TextWord *word1, const TextWord *word2);

    void visitSelection(TextSelectionVisitor *visitor, const PDFRectangle &selection);

    // Get the TextFontInfo object associated with a character.
    const TextFontInfo *getFontInfo(int idx) const { return chars[idx].font; }

    // Get the next TextWord on the linked list.
    const TextWord *getNext() const { return next; }

    int getLength() const { return chars.size(); }
    const Unicode *getChar(int idx) const { return &chars[idx].text; }
    // Text of the word in the current text encoding (globalParams->getTextEncoding()).
    // This looks the encoding up (taking a lock) on every call: when converting many
    // words, get the UnicodeMap once and use the overload below.
    std::unique_ptr<std::string> getText() const;
    // Text of the word in the encoding given by <uMap> (null gives an empty string).
    std::unique_ptr<std::string> getText(const UnicodeMap *uMap) const;
    // Like getText(uMap), but a word containing right-to-left characters (stored in visual
    // order) is returned in logical order, as the plain text output orders it, without
    // bidi embedding controls.
    std::unique_ptr<std::string> getLogicalText(const UnicodeMap *uMap) const;
    const GooString *getFontName(int idx) const { return chars[idx].font->fontName; }
    void getColor(double *r, double *g, double *b) const
    {
        *r = colorR;
        *g = colorG;
        *b = colorB;
    }
    void getBBox(double *xMinA, double *yMinA, double *xMaxA, double *yMaxA) const
    {
        *xMinA = xMin;
        *yMinA = yMin;
        *xMaxA = xMax;
        *yMaxA = yMax;
    }
    PDFRectangle getBBox() const { return { xMin, yMin, xMax, yMax }; }

    void getCharBBox(int charIdx, double *xMinA, double *yMinA, double *xMaxA, double *yMaxA) const;
    double getFontSize() const { return fontSize; }
    int getRotation() const { return rot; }
    int getCharPos() const { return chars.empty() ? 0 : chars.front().charPos; }
    int getCharLen() const { return chars.empty() ? 0 : chars.back().charPos - chars.front().charPos; }
    bool getSpaceAfter() const { return spaceAfter; }
    bool isUnderlined() const { return underlined; }
    // True if the word is invisible text (render mode 3), such as an OCR text layer.
    bool isInvisible() const { return invisible; }
    const AnnotLink *getLink() const { return link; }
    double getEdge(int i) const { return chars[i].edge; }
    // Per-character facts for glyph-level output (pdftotext -glyphs): the font-local char
    // code, the content-stream position (characters from one glyph share it), whether the
    // character came from an /ActualText span, and the end edge of the last character.
    CharCode getCharCode(int i) const { return chars[i].charcode; }
    int getCharPosAt(int i) const { return chars[i].charPos; }
    bool isActualTextChar(int i) const { return chars[i].actualText; }
    double getEdgeEnd() const { return edgeEnd; }
    double getAdvance(int i) const { return chars[i].advance; }
    // The character's text matrix: font transform (m[0..3]) and origin in page space (m[4], m[5]).
    const Matrix &getTextMat(int i) const { return chars[i].textMat; }
    double getBaseline() const { return base; }
    bool hasSpaceAfter() const { return spaceAfter; }
    const TextWord *nextWord() const { return next; };
    auto len() const { return chars.size(); }

private:
    void setInitialBounds(TextFontInfo *fontA, double x, double y);

    int rot; // rotation, multiple of 90 degrees
             //   (0, 1, 2, or 3)
    GfxFont::WritingMode wMode;
    double xMin, xMax; // bounding box x coordinates
    double yMin, yMax; // bounding box y coordinates
    double base; // baseline x or y coordinate

    double fontSize; // font size

    struct CharInfo
    {
        Unicode text;
        CharCode charcode;
        int charPos;
        double edge;
        TextFontInfo *font;
        Matrix textMat;
        bool actualText = false; // replacement text from a marked-content /ActualText span
        double advance = 0; // advance along the primary axis (0 for merged combining marks)
    };
    std::vector<CharInfo> chars;
    int charPosEnd = 0;
    double edgeEnd = 0;

    bool spaceAfter; // set if there is a space between this
                     //   word and the next word on the line
    bool underlined;
    bool invisible; // whether we are invisible (glyphless)
    bool shown = false; // a glyph of it is not render mode 3 and has its centre inside the clip
    TextWord *next; // next word in line

    double colorR, // word color
            colorG, colorB;

    AnnotLink *link;

    friend class TextPool;
    friend class TextLine;
    friend class TextBlock;
    friend class TextFlow;
    friend class TextWordList;
    friend class TextPage;

    friend class TextSelectionPainter;
    friend class TextSelectionDumper;
};

//------------------------------------------------------------------------
// TextPool
//------------------------------------------------------------------------

class TextPool
{
public:
    TextPool();
    ~TextPool();

    TextPool(const TextPool &) = delete;
    TextPool &operator=(const TextPool &) = delete;

    TextWord *getPool(int baseIdx) { return pool[baseIdx - minBaseIdx].head; }
    void setPool(int baseIdx, TextWord *p) { pool[baseIdx - minBaseIdx].head = p; }

    int getBaseIdx(double base) const;

    void addWord(TextWord *word);
    void sort();

private:
    int minBaseIdx; // min baseline bucket index
    int maxBaseIdx; // max baseline bucket index
    struct WordList
    {
        TextWord *head = nullptr;
        TextWord *tail = nullptr;
    };
    std::vector<WordList> pool;

    friend class TextBlock;
    friend class TextPage;
};

struct TextFlowData;

//------------------------------------------------------------------------
// TextLine
//------------------------------------------------------------------------

class TextLine
{
public:
    TextLine(TextBlock *blkA, int rotA, double baseA);
    ~TextLine();

    TextLine(const TextLine &) = delete;
    TextLine &operator=(const TextLine &) = delete;

    void addWord(TextWord *word);

    // Return the distance along the primary axis between <this> and
    // <line>.
    double primaryDelta(const TextLine *line) const;

    // Compares <this> to <line>, returning -1 (<), 0 (=), or +1 (>),
    // based on a primary-axis comparison, e.g., x ordering if rot=0.
    int primaryCmp(const TextLine *line) const;

    // Compares <this> to <line>, returning -1 (<), 0 (=), or +1 (>),
    // based on a secondary-axis comparison of the baselines, e.g., y
    // ordering if rot=0.
    int secondaryCmp(const TextLine *line) const;

    int cmpYX(const TextLine *line) const;

    static bool cmpXY(const TextLine *line1, const TextLine *line2);

    void coalesce(const UnicodeMap *uMap);

    void visitSelection(TextSelectionVisitor *visitor, const PDFRectangle &selection, SelectionStyle style);
    PDFRectangle getBBox() const { return { xMin, yMin, xMax, yMax }; }

    // Get the head of the linked list of TextWords.
    const TextWord *getWords() const { return words; }

    // Get the next TextLine on the linked list.
    const TextLine *getNext() const { return next; }

private:
    std::pair<int, int> getLineBounds(const PDFRectangle &area) const;

    TextBlock *blk; // parent block
    int rot; // text rotation
    double xMin, xMax; // bounding box x coordinates
    double yMin, yMax; // bounding box y coordinates
    double base; // baseline x or y coordinate
    TextWord *words; // words in this line
    TextWord *lastWord; // last word in this line
    Unicode *text; // Unicode text of the line, including
                   //   spaces between words
    double *edge; // "near" edge x or y coord of each char
                  //   (plus one extra entry for the last char)
    int *col; // starting column number of each Unicode char
    int len; // number of Unicode chars
    int convertedLen; // total number of converted characters
    TextLine *next; // next line in block
    Unicode *normalized; // normalized form of Unicode text
    int normalized_len; // number of normalized Unicode chars
    int *normalized_idx; // indices of normalized chars into Unicode text
    Unicode *ascii_translation; // ascii translation from the normalized text
    int ascii_len; // length of ascii translation text
    int *ascii_idx; // indices of ascii chars into Unicode text of line

    friend class TextLineFrag;
    friend class TextBlock;
    friend class TextFlow;
    friend class TextWordList;
    friend class TextPage;

    friend class TextSelectionPainter;
    friend class TextSelectionSizer;
    friend class TextSelectionDumper;
};

//------------------------------------------------------------------------
// TextBlock
//------------------------------------------------------------------------

class TextBlock
{
public:
    TextBlock(TextPage *pageA, int rotA);
    ~TextBlock();

    TextBlock(const TextBlock &) = delete;
    TextBlock &operator=(const TextBlock &) = delete;

    void addWord(TextWord *word);

    void coalesce(const UnicodeMap *uMap, double fixedPitch);

    // Update this block's priMin and priMax values, looking at <blk>.
    void updatePriMinMax(const TextBlock *blk);

    static bool cmpXYPrimaryRot(const TextBlock *blk1, const TextBlock *blk2);

    int primaryCmp(const TextBlock *blk) const;

    double secondaryDelta(const TextBlock *blk) const;

    // Returns true if <this> is below <blk>, relative to the page's
    // primary rotation.
    bool isBelow(const TextBlock *blk) const;

    void visitSelection(TextSelectionVisitor *visitor, const PDFRectangle &selection, SelectionStyle style);

    // Get the head of the linked list of TextLines.
    const TextLine *getLines() const { return lines; }

    // Get the next TextBlock on the linked list.
    const TextBlock *getNext() const { return next; }

    void getBBox(double *xMinA, double *yMinA, double *xMaxA, double *yMaxA) const
    {
        *xMinA = xMin;
        *yMinA = yMin;
        *xMaxA = xMax;
        *yMaxA = yMax;
    }
    PDFRectangle getBBox() const { return { xMin, yMin, xMax, yMax }; }

    int getLineCount() const { return nLines; }

private:
    bool isBeforeByRule1(const TextBlock *blk1);
    bool isBeforeByRepeatedRule1(const TextBlock *blkList, const TextBlock *blk1);
    bool isBeforeByRule2(const TextBlock *blk1);

    int visitDepthFirst(TextBlock *blkList, int pos1, TextBlock **sorted, int sortPos, bool *visited);
    int visitDepthFirst(TextBlock *blkList, int pos1, TextBlock **sorted, int sortPos, bool *visited, TextBlock **cache, int cacheSize);

    TextPage *page; // the parent page
    int rot; // text rotation
    double xMin, xMax; // bounding box x coordinates
    double yMin, yMax; // bounding box y coordinates
    double priMin, priMax; // whitespace bounding box along primary axis
    double ExMin, ExMax; // extended bounding box x coordinates
    double EyMin, EyMax; // extended bounding box y coordinates
    int tableId; // id of table to which this block belongs
    bool tableEnd; // is this block at end of line of actual table

    TextPool *pool; // pool of words (used only until lines
                    //   are built)
    TextLine *lines; // linked list of lines
    TextLine *curLine; // most recently added line
    int nLines; // number of lines
    int charCount; // number of characters in the block
    int col; // starting column
    int nColumns; // number of columns in the block

    TextBlock *next;
    TextBlock *stackNext;

    friend class TextLine;
    friend class TextLineFrag;
    friend class TextFlow;
    friend class TextWordList;
    friend class TextPage;
    friend class TextSelectionPainter;
    friend class TextSelectionDumper;
};

//------------------------------------------------------------------------
// TextFlow
//------------------------------------------------------------------------

class TextFlow
{
public:
    TextFlow(TextPage *pageA, TextBlock *blk);
    ~TextFlow();

    TextFlow(const TextFlow &) = delete;
    TextFlow &operator=(const TextFlow &) = delete;

    // Add a block to the end of this flow.
    void addBlock(TextBlock *blk);

    // Returns true if <blk> fits in the flow, i.e., (1)
    // it uses a font no larger than the last block added to the flow,
    // and (2) it fits within the flow's [priMin, priMax] along the
    // primary axis.
    bool blockFits(const TextBlock *blk) const;

    // Get the head of the linked list of TextBlocks.
    const TextBlock *getBlocks() const { return blocks; }

    // Get the next TextFlow on the linked list.
    const TextFlow *getNext() const { return next; }

private:
    TextPage *page; // the parent page
    double xMin, xMax; // bounding box x coordinates
    double yMin, yMax; // bounding box y coordinates
    double priMin, priMax; // whitespace bounding box along primary axis
    TextBlock *blocks; // blocks in flow
    TextBlock *lastBlk; // last block in this flow
    TextFlow *next;

    friend class TextWordList;
    friend class TextPage;
};

//------------------------------------------------------------------------
// TextWordList
//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT TextWordList
{
public:
    // Build a flat word list, in content stream order (if
    // text->rawOrder is true), physical layout order (if <physLayout>
    // is true and text->rawOrder is false), or reading order (if both
    // flags are false).
    TextWordList(const TextPage *text, bool physLayout);

    ~TextWordList();

    TextWordList(const TextWordList &) = delete;
    TextWordList &operator=(const TextWordList &) = delete;

    const std::vector<TextWord *> &getWords() const { return words; }

private:
    std::vector<TextWord *> words;
};

class TextWordSelection
{
public:
    TextWordSelection(const TextWord *wordA, int beginA, int endA) : word(wordA), begin(beginA), end(endA) { }

    const TextWord *getWord() const { return word; }
    int getBegin() const { return begin; }
    int getEnd() const { return end; }

    // True if this is the last word of a table cell that is followed by
    // another cell of the same table row in the selected text. A cell
    // separator (a tab) is emitted after it in the selection text.
    bool endsTableCell() const { return cellEnd; }

    // True if one character (a space or a table cell separator) follows this
    // word in the selection text when another word follows it on the line.
    bool hasSeparatorAfter() const { return word->hasSpaceAfter() || cellEnd; }

private:
    const TextWord *word;
    int begin;
    int end;
    bool cellStart = false; // first word of a cell that follows another cell in its row
    bool cellEnd = false; // last word of a cell that precedes another cell in its row

    friend class TextSelectionPainter;
    friend class TextSelectionDumper;
};

//------------------------------------------------------------------------
// TextPage
//------------------------------------------------------------------------

// An axis-aligned vector rule (line segment or thin filled rectangle) or filled box,
// in the same coordinate space as the page's words (see TextOutputDev::setCollectRules).
struct TextRule
{
    double xMin, yMin, xMax, yMax;
    double r, g, b; // stroke or fill colour, 0..1
    bool box; // a filled rectangle that is not thin enough to be a rule
    bool imageMask = false; // a thin stencil image mask (TextOutputDev::setCollectImageMaskRules)
    bool pastDocumentCap = false; // kept for page facts only: the document's budget was spent
};

// A visible image (or stencil mask) in the same coordinate space as the page's words (see
// TextOutputDev::setCollectImages): the part of its unit square inside the clip and the page.
struct TextImage
{
    double xMin, yMin, xMax, yMax;
    int pixelWidth, pixelHeight; // the image's own size in samples
    bool stencil; // an image mask
    Ref ref = Ref::INVALID(); // the image XObject, or INVALID for an inline image
    ImageDictInfo info {}; // what its dictionary says (filter, bits, colour space, masks)
    double dpiX = 0, dpiY = 0; // samples per inch along the image's own axes
    bool pastDocumentCap = false; // kept for page facts and scans only: the document's budget was spent
};

// A cluster of painted vector drawing that is not axis-aligned (see
// TextOutputDev::setCollectImages): curves and diagonal segments, the strokes of charts and
// diagrams. Paths made only of axis-aligned segments (rules, boxes, table grids, underlines,
// frames) are not counted. The box is the union of the paths' clipped extents.
struct TextGraphic
{
    double xMin, yMin, xMax, yMax;
    size_t paths = 0; // painted paths (a fill and a stroke of one path count twice)
    size_t segments = 0; // diagonal straight segments plus curve segments
    size_t curves = 0; // Bezier curve segments
    unsigned directions = 0; // bit k: a diagonal segment at an angle in the k-th of 8 sectors of 180 degrees
};

class POPPLER_PRIVATE_EXPORT TextPage
{
public:
    // Constructor.
    explicit TextPage(bool rawOrderA, bool discardDiagA = false);

    TextPage(const TextPage &) = delete;
    TextPage &operator=(const TextPage &) = delete;
    // Destructor.
    ~TextPage();

    // Start a new page.
    void startPage(const GfxState *state);

    // End the current page.
    void endPage();

    // Update the current font.
    void updateFont(const GfxState *state);

    // Begin a new word.
    void beginWord(const GfxState *state);

    // Add a character to the current word.
    void addChar(const GfxState *state, double x, double y, double dx, double dy, CharCode c, int nBytes, const Unicode *u, int uLen);

    // Add <nChars> invisible characters.
    void incCharCount(int nChars);

    // Mark the characters added until the next call as /ActualText replacement text.
    void setActualTextChars(bool on) { actualTextChars = on; }

    // Glyphs that text extraction drops because their Unicode is whitespace or NUL although
    // the font names them as something else (a TeX extension glyph whose raw code is 0x0C,
    // say). Recorded only with setCollectDroppedGlyphs, at most maxDroppedGlyphs per page.
    struct DroppedGlyph
    {
        double x, y, advance, fontSize;
        CharCode code;
        const TextFontInfo *font;
    };
    void setCollectDroppedGlyphs(bool on) { collectDropped = on; }
    const std::vector<DroppedGlyph> &getDroppedGlyphs() const { return dropped; }
    static constexpr size_t maxDroppedGlyphs = 20000;

    // End the current word, sorting it into the list of words.
    void endWord();

    // Add a word, sorting it into the list of words.
    void addWord(TextWord *word);

    // Add a (potential) underline.
    void addUnderline(double x0, double y0, double x1, double y1);

    // Add a visible rule (TextOutputDev::setCollectRules); at most maxRules per
    // page are kept. Consume the caller's document budget even when this page
    // is full. Rules past the document budget are kept, marked pastDocumentCap
    // (page facts are page-local; the XHTML leaves them out), and make the page
    // incomplete (getRulesTruncated) as rules the page cap rejects do;
    // getRulesPageTruncated is only the page cap.
    void addRule(const TextRule &rule, size_t &remainingDocumentRules);
    const std::vector<TextRule> &getRules() const { return rules; }
    bool getRulesTruncated() const { return rulesTruncated; }
    bool getRulesPageTruncated() const { return rulesPageTruncated; }
    static constexpr size_t maxRules = 20000;

    // Add a visible image (TextOutputDev::setCollectImages); at most maxImages per page.
    // Images past the document budget are kept, marked pastDocumentCap, as for rules; they
    // follow every image within it on the page.
    void addImage(const TextImage &image, size_t &remainingDocumentImages);
    const std::vector<TextImage> &getImages() const { return images; }
    bool getImagesTruncated() const { return imagesTruncated; }
    bool getImagesPageTruncated() const { return imagesPageTruncated; }
    static constexpr size_t maxImages = 2000;

    // Add one painted path that is not axis-aligned (TextOutputDev::setCollectImages). The page
    // keeps a fixed grid of cells, each the summary of the paths centred in it, so memory and
    // time per path stay constant however many paths a page paints.
    void addGraphic(const TextGraphic &graphic);
    // The cells' summaries merged where their boxes touch (grown by gap): at most
    // graphicGrid * graphicGrid clusters, in top-to-bottom, left-to-right order of their first cell.
    // <budget> (optional) is the number of box pairs the merge may still compare, decremented as
    // it does; when it runs out the clusters are returned as merged so far and <budgetHit> set.
    std::vector<TextGraphic> getGraphics(double gap, size_t *budget = nullptr, bool *budgetHit = nullptr) const;
    // whether any drawing was summarised on the page (getGraphics would return some)
    bool hasGraphics() const;
    static constexpr int graphicGrid = 16;

    // Add a hyperlink.
    void addLink(int xMin, int yMin, int xMax, int yMax, AnnotLink *link);

    // DocSlurp page facts: the sink that sees each glyph and word (nullptr: none), and the
    // words added to this page so far (before coalescing removes duplicated text), with those
    // drawn invisibly (render mode 3).
    void setPageFacts(PageFacts *factsA) { facts = factsA; }
    size_t getWordCount() const { return wordsAdded; }
    size_t getInvisibleWordCount() const { return invisibleWordsAdded; }
    size_t getVisibleWordCount() const { return wordsAdded - invisibleWordsAdded; }

    // Free the words not laid out (TextOutputDev::setWordsOnly): coalesce moves them out of
    // the pools; without it they are deleted here.
    void discardWords();

    // Coalesce strings that look like parts of the same line.
    void coalesce(bool physLayout, double fixedPitch, bool doHTML);
    void coalesce(bool physLayout, double fixedPitch, bool doHTML, double minColSpacing1);

    // Find a string.  If <startAtTop> is true, starts looking at the
    // top of the page; else if <startAtLast> is true, starts looking
    // immediately after the last find result; else starts looking at
    // <xMin>,<yMin>.  If <stopAtBottom> is true, stops looking at the
    // bottom of the page; else if <stopAtLast> is true, stops looking
    // just before the last find result; else stops looking at
    // <xMax>,<yMax>.
    bool findText(const Unicode *s, int len, bool startAtTop, bool stopAtBottom, bool startAtLast, bool stopAtLast, bool caseSensitive, bool backward, bool wholeWord, double *xMin, double *yMin, double *xMax, double *yMax);

    // Adds new parameter ignoreDiacritics, which will do diacritics
    // insensitive search, i.e. ignore accents, umlauts, diaeresis,etc.
    // while matching. This option will be ignored if <s> contains characters
    // which are not pure ascii.
    bool findText(const Unicode *s, int len, bool startAtTop, bool stopAtBottom, bool startAtLast, bool stopAtLast, bool caseSensitive, bool ignoreDiacritics, bool backward, bool wholeWord, double *xMin, double *yMin, double *xMax,
                  double *yMax);

    // Adds new parameter <matchAcrossLines>, which allows <s> to match on text
    // spanning from end of a line to the next line. In that case, the rect for
    // the part of match that falls on the next line will be stored in
    // <continueMatch>, and if hyphenation (i.e. ignoring hyphen at end of line)
    // was used while matching at the end of the line prior to <continueMatch>,
    // then <ignoredHyphen> will be true, otherwise will be false.
    // Only finding across two lines is supported, i.e. it won't match where <s>
    // spans more than two lines.
    //
    // <matchAcrossLines> will be ignored if <backward> is true (as that
    // combination has not been implemented yet).
    bool findText(const Unicode *s, int len, bool startAtTop, bool stopAtBottom, bool startAtLast, bool stopAtLast, bool caseSensitive, bool ignoreDiacritics, bool matchAcrossLines, bool backward, bool wholeWord, double *xMin, double *yMin,
                  double *xMax, double *yMax, PDFRectangle *continueMatch, bool *ignoredHyphen);

    // Get the text which is inside the specified rectangle.
    // physical layout false and raw order false does not go well with a rectangle
    GooString getText(const std::optional<PDFRectangle> &area, EndOfLineKind textEOL, bool physLayout, EndOfLineHyphenMode hyphenMode) const;

    void visitSelection(TextSelectionVisitor *visitor, const PDFRectangle &selection, SelectionStyle style);

    void drawSelection(OutputDev *out, double scale, int rotation, const PDFRectangle &selection, SelectionStyle style, const GfxColor &glyph_color, const GfxColor &box_color, double box_opacity, bool draw_glyphs);

    std::vector<PDFRectangle *> *getSelectionRegion(const PDFRectangle &selection, SelectionStyle style, double scale);

    GooString getSelectionText(const PDFRectangle &selection, SelectionStyle style);

    [[nodiscard]] std::vector<std::vector<std::unique_ptr<TextWordSelection>>> getSelectionWords(const PDFRectangle &selection, SelectionStyle style);

    // Find a string by character position and length.  If found, sets
    // the text bounding rectangle and returns true; otherwise returns
    // false.
    bool findCharRange(int pos, int length, double *xMin, double *yMin, double *xMax, double *yMax) const;

    // Dump contents of page to a file.
    void dump(void *outputStream, TextOutputFunc outputFunc, bool physLayout, EndOfLineKind textEOL, bool pageBreaks, bool suppressLastEol, std::optional<PDFRectangle> area, EndOfLineHyphenMode hyphenMode) const;

    // Get the head of the linked list of TextFlows.
    const TextFlow *getFlows() const { return flows; }

    // Size of the last page, in the same (rotated, scaled) device space as
    // the word coordinates.
    double getPageWidth() const { return pageWidth; }
    double getPageHeight() const { return pageHeight; }

    // If true, will combine characters when a base and combining
    // character are drawn on eachother.
    void setMergeCombining(bool merge);

    void setInlineLinkURIs(bool inlineLinkURIsA);

    // Build a flat word list, in content stream order (if
    // this->rawOrder is true), physical layout order (if <physLayout>
    // is true and this->rawOrder is false), or reading order (if both
    // flags are false).
    std::unique_ptr<TextWordList> makeWordList(bool physLayout);

private:
    void clear();
    static void assignColumns(TextLineFrag *frags, int nFrags, bool rot);
    int dumpFragment(const Unicode *text, int len, const UnicodeMap *uMap, GooString *s) const;
    static void adjustRotation(TextLine *line, int start, int end, double *xMin, double *xMax, double *yMin, double *yMax);

    bool rawOrder; // keep text in content stream order
    bool discardDiag; // discard diagonal text
    bool inlineLinkURIs = false;
    bool mergeCombining; // merge when combining and base characters
                         // are drawn on top of each other

    double pageWidth, pageHeight; // width and height of current page
    TextWord *curWord; // currently active string
    int charPos; // next character position (within content
                 //   stream)
    TextFontInfo *curFont; // current font
    double curFontSize; // current font size
    int nest; // current nesting level (for Type 3 fonts)
    int nTinyChars; // number of "tiny" chars seen so far
    bool lastCharOverlap; // set if the last added char overlapped the
                          //   previous char
    bool diagonal; // whether the current text is diagonal

    std::unique_ptr<TextPool> pools[4]; // a "pool" of TextWords for each rotation
    TextFlow *flows; // linked list of flows
    TextBlock **blocks; // array of blocks, in yx order
    int nBlocks; // number of blocks
    int primaryRot; // primary rotation
    bool primaryLR; // primary direction (true means L-to-R,
                    //   false means R-to-L)
    TextWord *rawWords; // list of words, in raw order (only if
                        //   rawOrder is set)
    TextWord *rawLastWord; // last word on rawWords list

    std::vector<std::unique_ptr<TextFontInfo>> fonts; // all font info objects used on this page

    double lastFindXMin, // coordinates of the last "find" result
            lastFindYMin;
    bool haveLastFind;

    std::vector<std::unique_ptr<TextUnderline>> underlines;
    std::vector<TextRule> rules;
    bool rulesTruncated = false, rulesPageTruncated = false;
    std::vector<TextImage> images;
    bool imagesTruncated = false, imagesPageTruncated = false;
    std::array<TextGraphic, graphicGrid * graphicGrid> graphicCells {};
    bool actualTextChars = false;
    bool collectDropped = false;
    std::vector<DroppedGlyph> dropped;
    void recordDropped(const GfxState *state, double x1, double y1, double w1, CharCode c);
    std::vector<std::unique_ptr<TextLink>> links;
    PageFacts *facts = nullptr;
    size_t wordsAdded = 0, invisibleWordsAdded = 0;

    friend class TextLine;
    friend class TextLineFrag;
    friend class TextBlock;
    friend class TextFlow;
    friend class TextWordList;
    friend class TextSelectionPainter;
    friend class TextSelectionDumper;
};

//------------------------------------------------------------------------
// ActualText
//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT ActualText
{
public:
    /// Create an ActualText
    /// \note: \param out must be kept alive during the lifetime of this object
    explicit ActualText(TextPage *out);
    ~ActualText();

    ActualText(const ActualText &) = delete;
    ActualText &operator=(const ActualText &) = delete;

    void addChar(const GfxState *state, double x, double y, double dx, double dy, CharCode c, int nBytes, const Unicode *u, int uLen);
    void begin(const GfxState *state, const std::string &text);
    void end(const GfxState *state);

private:
    TextPage *text;

    std::optional<std::string> actualText; // replacement text for the span
    double actualTextX0;
    double actualTextY0;
    double actualTextX1;
    double actualTextY1;
    int actualTextNBytes;
    int actualTextDepth; // nesting level of ActualText spans
};

//------------------------------------------------------------------------
// TextOutputDev
//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT TextOutputDev : public OutputDev
{
public:
    static double minColSpacing1_default;

    // Open a text output file.  If <fileName> is NULL, no file is
    // written (this is useful, e.g., for searching text).  If
    // <physLayoutA> is true, the original physical layout of the text
    // is maintained.  If <rawOrder> is true, the text is kept in
    // content stream order.  If <discardDiag> is true, diagonal text
    // is removed from output.
    TextOutputDev(const char *fileName, bool physLayoutA, double fixedPitchA, bool rawOrderA, bool append, bool discardDiagA = false);

    // Create a TextOutputDev which will write to a generic stream.  If
    // <physLayoutA> is true, the original physical layout of the text
    // is maintained.  If <rawOrder> is true, the text is kept in
    // content stream order.  If <discardDiag> is true, diagonal text
    // is removed from output.
    TextOutputDev(TextOutputFunc func, void *stream, bool physLayoutA, double fixedPitchA, bool rawOrderA, bool discardDiagA = false);

    // Destructor.
    ~TextOutputDev() override;

    // Check if file was successfully created.
    virtual bool isOk() { return ok; }

    //---- get info about output device

    // Does this device use upside-down coordinates?
    // (Upside-down means (0,0) is the top left corner of the page.)
    bool upsideDown() override { return true; }

    // Does this device use drawChar() or drawString()?
    bool useDrawChar() override { return true; }

    // Does this device use beginType3Char/endType3Char?  Otherwise,
    // text in Type 3 fonts will be drawn with drawChar/drawString.
    bool interpretType3Chars() override { return false; }

    // Does this device need non-text content?
    bool needNonText() override { return false; }

    // Does this device require incCharCount to be called for text on
    // non-shown layers?
    bool needCharCount() override { return true; }

    //----- initialization and control

    bool checkPageSlice(Page *pageA, double hDPI, double vDPI, int rotate, bool useMediaBox, bool crop, int sliceX, int sliceY, int sliceW, int sliceH, bool printing, bool (*abortCheckCbk)(void *data) = nullptr,
                        void *abortCheckCbkData = nullptr, bool (*annotDisplayDecideCbk)(Annot *annot, void *user_data) = nullptr, void *annotDisplayDecideCbkData = nullptr) override;

    // Start a page.
    void startPage(int pageNum, GfxState *state, XRef *xref) override;

    // End a page.
    void endPage() override;

    //----- save/restore graphics state
    void restoreState(GfxState *state) override;

    //----- update text state
    void updateFont(GfxState *state) override;

    //----- text drawing
    void beginString(GfxState *state, const std::string &s) override;
    void endString(GfxState *state) override;
    void drawChar(GfxState *state, double x, double y, double dx, double dy, double originX, double originY, CharCode c, int nBytes, const Unicode *u, int uLen) override;
    void incCharCount(int nChars) override;
    void beginActualText(GfxState *state, const std::string &text) override;
    void endActualText(GfxState *state) override;

    //----- path painting
    void stroke(GfxState *state) override;
    void fill(GfxState *state) override;
    void eoFill(GfxState *state) override;

    //----- link borders
    void processLink(AnnotLink *link) override;

    //----- special access

    // Find a string.  If <startAtTop> is true, starts looking at the
    // top of the page; else if <startAtLast> is true, starts looking
    // immediately after the last find result; else starts looking at
    // <xMin>,<yMin>.  If <stopAtBottom> is true, stops looking at the
    // bottom of the page; else if <stopAtLast> is true, stops looking
    // just before the last find result; else stops looking at
    // <xMax>,<yMax>.
    bool findText(const Unicode *s, int len, bool startAtTop, bool stopAtBottom, bool startAtLast, bool stopAtLast, bool caseSensitive, bool backward, bool wholeWord, double *xMin, double *yMin, double *xMax, double *yMax) const;

    // Get the text which is inside the specified rectangle.
    // You can only give an area if either physLayout or rawOrder are true
    GooString getText(const std::optional<PDFRectangle> &area) const;

    // Find a string by character position and length.  If found, sets
    // the text bounding rectangle and returns true; otherwise returns
    // false.
    bool findCharRange(int pos, int length, double *xMin, double *yMin, double *xMax, double *yMax) const;

    void drawSelection(OutputDev *out, double scale, int rotation, const PDFRectangle &selection, SelectionStyle style, const GfxColor &glyph_color, const GfxColor &box_color, double box_opacity, bool draw_glyphs);

    std::vector<PDFRectangle *> *getSelectionRegion(const PDFRectangle &selection, SelectionStyle style, double scale);

    GooString getSelectionText(const PDFRectangle &selection, SelectionStyle style);

    // If true, will combine characters when a base and combining
    // character are drawn on eachother.
    void setMergeCombining(bool merge);

    // Build a flat word list, in content stream order (if
    // this->rawOrder is true), physical layout order (if
    // this->physLayout is true and this->rawOrder is false), or reading
    // order (if both flags are false).
    std::unique_ptr<TextWordList> makeWordList();

    // Returns the TextPage object for the last rasterized page,
    // transferring ownership to the caller.
    std::unique_ptr<TextPage> takeText();

    // Turn extra processing for HTML conversion on or off.
    void enableHTMLExtras(bool doHTMLA) { doHTML = doHTMLA; }

    // Record axis-aligned stroked segments, thin filled rectangles (rules) and filled
    // rectangles (boxes) of each page, for table borders and cell shading.
    void setCollectRules(bool collectRulesA) { collectRules = collectRulesA; }
    // With setCollectRules: also record thin axis-aligned stencil image masks as rules
    // (TextRule::imageMask). Ghostscript draws TeX rules (fraction bars) this way. Only the
    // image geometry is used; the image data is never decoded.
    void setCollectImageMaskRules(bool on) { collectImageMaskRules = on; }
    // Record named glyphs that extraction drops as whitespace (TextPage::DroppedGlyph).
    void setCollectDroppedGlyphs(bool on);
    const std::vector<TextPage::DroppedGlyph> &getDroppedGlyphs() const;
    bool needImageMaskGeometry() override { return collectRules && collectImageMaskRules; }
    void imageMaskGeometry(GfxState *state) override;
    const std::vector<TextRule> &getRules() const;
    bool getRulesTruncated() const;

    // Record where each visible image and image mask lands on the page (TextImage). Only the
    // image dictionary is read; the image data is never decoded.
    void setCollectImages(bool on) { collectImages = on; }
    bool needImageGeometry() override { return collectImages || facts; }
    void imageGeometry(GfxState *state, int width, int height, bool stencil, Ref ref, const ImageDictInfo &info) override;
    const std::vector<TextImage> &getImages() const;
    bool getImagesTruncated() const;
    // With setCollectImages: the clusters of painted drawing that is not axis-aligned on this
    // page (TextGraphic), at most maxDocGraphics over the document; call once per page.
    std::vector<TextGraphic> takeGraphics();
    bool getGraphicsTruncated() const { return graphicsTruncated; }

    void setInlineLinkURIs(bool inlineLinkURIsA);

    // DocSlurp page facts: compute them for each page into <factsA> (nullptr: off). The sink
    // must outlive the pages displayed with it. It turns on image geometry, and with vector
    // facts the rule and drawing summaries, without changing what is written for them.
    void setPageFacts(PageFacts *factsA);
    // Words only: endPage skips coalescing into lines, blocks and flows (no reading order, no
    // text output); for a caller that needs only page facts and word counts.
    void setWordsOnly(bool on) { wordsOnly = on; }
    GfxOpTally *getOpTally() override;
    ImageInfoCache *getImageInfoCache() override { return &imageInfoCache; }
    void beginAnnot(Annot *annot) override;
    void endAnnot(Annot *annot) override;

    // The scan verdict of the last page (see classifyScan) under <thresholds>.
    ScanVerdict getScan(const ScanThresholds &thresholds) const;

    // Get the head of the linked list of TextFlows for the
    // last rasterized page.
    const TextFlow *getFlows() const;

    // Size of the last rasterized page in the coordinate space of the word
    // bounding boxes: it honours the page /Rotate, the resolution, and
    // whether the media box or crop box was used.
    double getPageWidth() const;
    double getPageHeight() const;

    void setTextEOL(EndOfLineKind textEOLA) { textEOL = textEOLA; }
    void setTextPageBreaks(bool textPageBreaksA) { textPageBreaks = textPageBreaksA; }
    double getMinColSpacing1() const { return minColSpacing1; }
    void setMinColSpacing1(double val) { minColSpacing1 = val; }
    void setEndOfLineHyphenMode(EndOfLineHyphenMode mode) { hyphenMode = mode; }

private:
    TextOutputFunc outputFunc; // output function
    void *outputStream; // output stream
    bool needClose; // need to close the output file?
                    //   (only if outputStream is a FILE*)
    std::unique_ptr<TextPage> text; // text for the current page
    bool physLayout; // maintain original physical layout when
                     //   dumping text
    double fixedPitch; // if physLayout is true and this is non-zero,
                       //   assume fixed-pitch characters with this
                       //   width
    double minColSpacing1; // see default value defined with same name at TextOutputDev.cc
    bool rawOrder; // keep text in content stream order
    bool discardDiag; // Diagonal text, i.e., text that is not close to one of the
                      // 0, 90, 180, or 270 degree axes, is discarded. This is useful
                      // to skip watermarks drawn on top of body text, etc.
    bool doHTML; // extra processing for HTML conversion
    bool collectRules = false;
    bool collectImageMaskRules = false;
    bool collectImages = false;
    static constexpr size_t maxDocImages = 100000;
    size_t remainingDocImages = maxDocImages;
    static constexpr size_t maxDocGraphics = 100000;
    size_t remainingDocGraphics = maxDocGraphics;
    ImageInfoCache imageInfoCache; // DocSlurp: see getImageInfoCache
    bool graphicsTruncated = false;
    void collectPathGraphic(const GfxState *state, bool filled);
    static constexpr size_t maxDocRules = 200000;
    // Budget for visible rule candidates across pages, including candidates
    // rejected by a full page. TextPage consumes it but does not own its lifetime.
    size_t remainingDocRules = maxDocRules;
    void collectPathRules(const GfxState *state, bool filled);
    void addClippedRule(const GfxState *state, double x0, double y0, double x1, double y1, double r, double g, double b, bool box, bool imageMask = false);
    bool inlineLinkURIs = false;
    PageFacts *facts = nullptr;
    bool wordsOnly = false;
    Page *currentPage = nullptr;
    bool ok; // set up ok?
    bool textPageBreaks; // insert end-of-page markers?
    EndOfLineKind textEOL; // type of EOL marker to use
    EndOfLineHyphenMode hyphenMode = EndOfLineHyphenMode::RemoveAll;

    std::unique_ptr<ActualText> actualText;
};

#endif
