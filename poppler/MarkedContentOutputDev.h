//========================================================================
//
// MarkedContentOutputDev.h
//
// This file is licensed under the GPLv2 or later
//
// Copyright 2013 Igalia S.L.
// Copyright 2018-2021, 2024, 2025 Albert Astals Cid <aacid@kde.org>
// Copyright 2021, 2023 Adrian Johnson <ajohnson@redneon.com>
// Copyright 2022 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright 2025 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright 2026 Stefan Brüns <stefan.bruens@rwth-aachen.de>
//
//========================================================================

#ifndef MARKEDCONTENTOUTPUTDEV_H
#define MARKEDCONTENTOUTPUTDEV_H

#include "poppler_private_export.h"
#include "OutputDev.h"
#include "GfxState.h"
#include "GfxFont.h"
#include <map>
#include <utility>
#include <vector>

class Dict;
class UnicodeMap;

class TextSpan
{
public:
    TextSpan(const TextSpan &other) = default;

    TextSpan &operator=(const TextSpan &other) = default;

    ~TextSpan() = default;

    const std::shared_ptr<GfxFont> &getFont() const { return data->font; }
    const GooString *getText() const { return data->text.get(); }
    GfxRGB &getColor() const { return data->color; }

private:
    // Note: Takes ownership of strings, increases refcount for font.
    TextSpan(std::unique_ptr<GooString> text, std::shared_ptr<GfxFont> font, const GfxRGB color) : data(std::make_shared<Data>())
    {
        data->text = std::move(text);
        data->font = std::move(font);
        data->color = color;
    }

    struct Data
    {
        std::shared_ptr<GfxFont> font;
        std::unique_ptr<const GooString> text;
        GfxRGB color;

        Data() = default;

        ~Data() = default;

        Data(const Data &) = delete;
        Data &operator=(const Data &) = delete;
    };

    std::shared_ptr<Data> data;

    friend class MarkedContentOutputDev;
};

using TextSpanArray = std::vector<TextSpan>;

class POPPLER_PRIVATE_EXPORT MarkedContentOutputDev : public OutputDev
{
public:
    // Collects the text of the marked content sequence with MCID <mcidA>
    // found in the content stream <stmObj> (the page content if it is not
    // a reference to a form XObject).
    explicit MarkedContentOutputDev(int mcidA, const Object &stmObj);

    // Collects the text of every marked content sequence with an MCID in a
    // single pass, see getCollectedSpans().
    MarkedContentOutputDev();

    ~MarkedContentOutputDev() override;

    virtual bool isOk() { return true; }
    bool upsideDown() override { return true; }
    bool useDrawChar() override { return true; }
    bool interpretType3Chars() override { return false; }
    bool needNonText() override { return false; }
    bool needCharCount() override { return false; }

    void startPage(int pageNum, GfxState *state, XRef *xref) override;
    void endPage() override;

    void beginForm(Object * /* obj */, Ref id) override;
    void endForm(Object * /* obj */, Ref id) override;

    void drawChar(GfxState *state, double xx, double yy, double dx, double dy, double ox, double oy, CharCode c, int nBytes, const Unicode *u, int uLen) override;

    void beginMarkedContent(const std::string &name, Dict *properties) override;
    void endMarkedContent(GfxState *state) override;

    // Spans of the MCID given to the constructor.
    const TextSpanArray &getTextSpans() const;

    // Key of the text collected by the default constructor: the form XObject
    // containing the marked content (Ref::INVALID() for the page content) and
    // the MCID.
    using Key = std::pair<Ref, int>;
    using CollectedSpans = std::map<Key, TextSpanArray>;

    // Spans of all the MCIDs seen so far (default-constructed device only).
    CollectedSpans takeCollectedSpans();

private:
    // Text being gathered for one MCID.
    struct Accumulator
    {
        std::shared_ptr<GfxFont> currentFont;
        std::unique_ptr<GooString> currentText;
        GfxRGB currentColor;
        TextSpanArray textSpans;

        Accumulator() { currentColor.r = currentColor.g = currentColor.b = 0; }
    };

    static void endSpan(Accumulator &acc);
    bool inMarkedContent() const { return activeCount > 0; }
    bool contentStreamMatch();
    bool needFontChange(const Accumulator &acc, const std::shared_ptr<const GfxFont> &font) const;

    bool collectAll;
    Accumulator single; // used when collecting a single MCID
    std::map<Key, Accumulator> accumulators; // used when collecting all MCIDs
    // One entry per open marked content sequence; nullptr for sequences
    // which do not collect text (no MCID, or nested in a collecting one).
    std::vector<Accumulator *> markStack;
    int activeCount = 0; // number of non-null entries in markStack
    int mcid = -1;
    std::vector<Ref> formStack;
    double pageWidth = 0.0;
    double pageHeight = 0.0;
    const UnicodeMap *unicodeMap = nullptr;
    Object stmRef;
};

#endif /* !MARKEDCONTENTOUTPUTDEV_H */
