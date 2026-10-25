//========================================================================
//
// MarkedContentOutputDev.cc
//
// This file is licensed under the GPLv2 or later
//
// Copyright 2013 Igalia S.L.
// Copyright 2018-2020, 2022, 2025 Albert Astals Cid <aacid@kde.org>
// Copyright 2021, 2023 Adrian Johnson <ajohnson@redneon.com>
// Copyright 2022 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright 2025 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright (C) 2025 Jonathan Hähne <jonathan.haehne@hotmail.com>
// Copyright 2026 Stefan Brüns <stefan.bruens@rwth-aachen.de>
//
//========================================================================

#include "MarkedContentOutputDev.h"
#include "GlobalParams.h"
#include "UnicodeMap.h"
#include "GfxState.h"
#include "GfxFont.h"
#include "Annot.h"
#include <algorithm>
#include <cmath>
#include <vector>

MarkedContentOutputDev::MarkedContentOutputDev(int mcidA, const Object &stmObj) : collectAll(false), mcid(mcidA)
{
    stmRef = stmObj.copy();
}

MarkedContentOutputDev::MarkedContentOutputDev() : collectAll(true) { }

MarkedContentOutputDev::~MarkedContentOutputDev() = default;

void MarkedContentOutputDev::endSpan(Accumulator &acc)
{
    if (acc.currentText && !acc.currentText->empty()) {
        // The TextSpan takes ownership of currentText and
        // increases the reference count for currentFont.
        acc.textSpans.push_back(TextSpan(std::move(acc.currentText), acc.currentFont, acc.currentColor));
    }
    acc.currentText.reset();
}

MarkedContentOutputDev::CollectedSpans MarkedContentOutputDev::takeCollectedSpans()
{
    CollectedSpans result;
    for (auto &[key, acc] : accumulators) {
        endSpan(acc);
        if (!acc.textSpans.empty()) {
            result.emplace(key, std::move(acc.textSpans));
        }
    }
    accumulators.clear();
    markStack.clear();
    activeCount = 0;
    return result;
}

void MarkedContentOutputDev::startPage(int /*pageNum*/, GfxState *state, XRef * /*xref*/)
{
    if (state) {
        pageWidth = state->getPageWidth();
        pageHeight = state->getPageHeight();
    } else {
        pageWidth = pageHeight = 0.0;
    }
}

void MarkedContentOutputDev::endPage()
{
    pageWidth = pageHeight = 0.0;
}

void MarkedContentOutputDev::beginForm(Object * /* obj */, Ref id)
{
    formStack.push_back(id);
}

void MarkedContentOutputDev::endForm(Object * /* obj */, Ref /*id*/)
{
    formStack.pop_back();
}

bool MarkedContentOutputDev::contentStreamMatch()
{
    if (stmRef.isRef()) {
        if (formStack.empty()) {
            return false;
        }
        return formStack.back() == stmRef.getRef();
    }
    return formStack.empty();
}

void MarkedContentOutputDev::beginMarkedContent(const std::string & /*name*/, Dict *properties)
{
    int id = -1;
    if (properties) {
        properties->lookupInt("MCID", {}, &id);
    }

    // Every begin is recorded so that it can be paired with its end, even
    // when it carries no MCID. Otherwise the end of nested marked content
    // without MCID (e.g. /Span, /OC) would pop the enclosing MCID.
    Accumulator *acc = nullptr;
    if (id != -1) {
        if (collectAll) {
            acc = &accumulators[Key(formStack.empty() ? Ref::INVALID() : formStack.back(), id)];
        } else if (id == mcid && contentStreamMatch()) {
            acc = &single;
        }
        // Text of nested marked content is already added to the enclosing
        // one, avoid adding it twice if both refer to the same MCID.
        if (acc && std::find(markStack.begin(), markStack.end(), acc) != markStack.end()) {
            acc = nullptr;
        }
        // In single MCID mode text is collected only for the outermost match
        if (acc && !collectAll && inMarkedContent()) {
            acc = nullptr;
        }
    }
    markStack.push_back(acc);
    if (acc) {
        activeCount++;
    }
}

void MarkedContentOutputDev::endMarkedContent(GfxState * /*state*/)
{
    if (markStack.empty()) {
        return;
    }

    Accumulator *acc = markStack.back();
    markStack.pop_back();
    if (acc) {
        activeCount--;
        // The marked content sequence ended, ensure that the last piece
        // of text collected ends up in a TextSpan.
        endSpan(*acc);
    }
}

bool MarkedContentOutputDev::needFontChange(const Accumulator &acc, const std::shared_ptr<const GfxFont> &font) const
{
    const std::shared_ptr<GfxFont> &currentFont = acc.currentFont;
    if (currentFont == font) {
        return false;
    }

    if (!currentFont) {
        return font != nullptr && font->isOk();
    }

    if (font == nullptr) {
        return true;
    }

    // Two non-null valid fonts are the same if they point to the same Ref
    if (*currentFont->getID() == *font->getID()) {
        return false;
    }

    return true;
}

void MarkedContentOutputDev::drawChar(GfxState *state, double xx, double yy, double dx, double dy, double /*ox*/, double /*oy*/, CharCode c, int /*nBytes*/, const Unicode *u, int uLen)
{
    if (!inMarkedContent() || !uLen) {
        return;
    }

    // Color changes are tracked here so the color can be chosen depending on
    // the render mode (for mode 1 stroke color is used), so there is no need
    // to implement both updateFillColor() and updateStrokeColor().
    GfxRGB color;
    if ((state->getRender() & 3) == 1) {
        state->getStrokeRGB(&color);
    } else {
        state->getFillRGB(&color);
    }

    const std::shared_ptr<GfxFont> font = state->getFont();

    // Save a span with the current changes, and perform the color/font
    // changes, in every sequence collecting text.
    for (Accumulator *acc : markStack) {
        if (!acc) {
            continue;
        }
        const bool colorChange = (color.r != acc->currentColor.r || color.g != acc->currentColor.g || color.b != acc->currentColor.b);

        // Check also for font changes.
        bool fontChange = needFontChange(*acc, font);

        if (colorChange || fontChange) {
            endSpan(*acc);
        }

        if (colorChange) {
            acc->currentColor = color;
        }

        if (fontChange) {
            acc->currentFont = font;
        }
    }

    double sp, dx2, dy2, w1, h1, x1, y1;

    // Subtract char and word spacing from the (dx,dy) values
    sp = state->getCharSpace();
    if (c == static_cast<CharCode>(0x20)) {
        sp += state->getWordSpace();
    }
    state->textTransformDelta(sp * state->getHorizScaling(), 0, &dx2, &dy2);
    dx -= dx2;
    dy -= dy2;
    state->transformDelta(dx, dy, &w1, &h1);
    state->transform(xx, yy, &x1, &y1);

    // Throw away characters that are not inside the page boundaries.
    if (x1 + w1 < 0 || x1 > pageWidth || y1 + h1 < 0 || y1 > pageHeight) {
        return;
    }

    if (std::isnan(x1) || std::isnan(y1) || std::isnan(w1) || std::isnan(h1)) {
        return;
    }

    std::string utf8;
    for (int i = 0; i < uLen; i++) {
        // Soft hyphen markers are skipped, as they are invisible unless
        // rendering is done to an actual device and the hyphenation hint
        // used. MarkedContentOutputDev extracts the *visible* text content.
        if (u[i] != 0x00AD) {
            // Add the UTF-8 sequence to the current text span.
            if (!unicodeMap) {
                unicodeMap = globalParams->getTextEncoding();
            }

            char buf[8];
            int n = unicodeMap->mapUnicode(u[i], buf, sizeof(buf));
            if (n > 0) {
                utf8.append(buf, n);
            }
        }
    }

    if (utf8.empty()) {
        return;
    }
    for (Accumulator *acc : markStack) {
        if (acc) {
            if (acc->currentText == nullptr) {
                acc->currentText = std::make_unique<GooString>();
            }
            acc->currentText->append(utf8);
        }
    }
}

const TextSpanArray &MarkedContentOutputDev::getTextSpans() const
{
    return single.textSpans;
}
