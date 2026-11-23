// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include "TextOutputDev.h"
#include <map>
#include <vector>

// Optional text extraction for Identity-CID subsets which have lost ToUnicode.
// Cache only verified GID-to-Unicode pairs; a font name alone proves nothing.
class RecoveringTextOutputDev final : public TextOutputDev
{
public:
    using TextOutputDev::TextOutputDev;
    void enableFontRecovery(XRef *xref) { recoveryXRef = xref; }
    void drawChar(GfxState *state, double x, double y, double dx, double dy, double ox, double oy, CharCode code, int nBytes, const Unicode *u, int uLen) override;

private:
    XRef *recoveryXRef = nullptr;
    std::map<Ref, std::vector<Unicode>> maps;
    size_t bytesLeft = 32 * 1024 * 1024;
    unsigned glyphsLeft = 200000;
    std::vector<Unicode> recover(const GfxFont &font);
};
