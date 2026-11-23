// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include "TextOutputDev.h"
#include "Type3FontRecovery.h"
#include <map>
#include <vector>

// Optional text extraction from embedded glyph names and verified outlines.
class RecoveringTextOutputDev final : public TextOutputDev
{
public:
    using TextOutputDev::TextOutputDev;
    void enableFontRecovery(XRef *xref) { recoveryXRef = xref; }
    void drawChar(GfxState *state, double x, double y, double dx, double dy, double ox, double oy, CharCode code, int nBytes, const Unicode *u, int uLen) override;

private:
    Type3FontRecovery type3;
    XRef *recoveryXRef = nullptr;
    std::map<Ref, std::vector<Unicode>> maps;
    // Reuse the installed CFF outline index across embedded subsets of a face.
    struct Candidate
    {
        unsigned gid;
        Unicode unicode;
    };
    std::map<std::pair<std::string, int>, std::map<uint64_t, Candidate>> cffOutlines;
    std::map<std::pair<std::string, int>, std::map<unsigned, Candidate>> cffCharacters;
    // Character-map entries indexed once per face for small symbolic TrueType subsets.
    std::map<std::pair<std::string, int>, std::map<uint64_t, std::vector<Candidate>>> symbolOutlines;
    size_t mapSlotsLeft = 64 * 65536; // Keep the previous maximum number of cached Unicode entries.
    size_t bytesLeft = 32 * 1024 * 1024;
    size_t recordBytesLeft = 32 * 1024 * 1024;
    unsigned comparisonsLeft = 200000;
    unsigned glyphsLeft = 200000;
    std::vector<Unicode> recover(const GfxFont &font);
};
