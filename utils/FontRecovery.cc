// SPDX-License-Identifier: GPL-2.0-or-later
#include "FontRecovery.h"
#include "GfxFont.h"
#include "GfxState.h"
#include "GlobalParams.h"
#include "XRef.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include <algorithm>
#include <array>

namespace {
struct Faces
{
    FT_Library library = nullptr;
    FT_Face embedded = nullptr;
    FT_Face installed = nullptr;
    ~Faces()
    {
        if (installed)
            FT_Done_Face(installed);
        if (embedded)
            FT_Done_Face(embedded);
        if (library)
            FT_Done_FreeType(library);
    }
};

constexpr auto loadFlags = FT_LOAD_NO_SCALE | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP | FT_LOAD_IGNORE_TRANSFORM;

bool sameGlyph(const Faces &f, unsigned embeddedGid, unsigned installedGid, Unicode u)
{
    if (FT_Load_Glyph(f.embedded, embeddedGid, loadFlags) || FT_Load_Glyph(f.installed, installedGid, loadFlags))
        return false;
    const auto a = f.embedded->glyph, b = f.installed->glyph;
    if (a->format != FT_GLYPH_FORMAT_OUTLINE || b->format != FT_GLYPH_FORMAT_OUTLINE || a->metrics.horiAdvance != b->metrics.horiAdvance)
        return false;
    const auto &x = a->outline, &y = b->outline;
    if (x.n_points != y.n_points || x.n_contours != y.n_contours)
        return false;
    if (x.n_points == 0)
        return u == ' ' && a->metrics.horiAdvance > 0;
    for (int i = 0; i < x.n_points; ++i) {
        if (x.points[i].x != y.points[i].x || x.points[i].y != y.points[i].y || FT_CURVE_TAG(x.tags[i]) != FT_CURVE_TAG(y.tags[i]))
            return false;
    }
    return std::equal(x.contours, x.contours + x.n_contours, y.contours);
}

// A fingerprint selects a candidate. sameGlyph verifies every coordinate and tag.
uint64_t outlineKey(FT_Face face, unsigned gid)
{
    if (FT_Load_Glyph(face, gid, loadFlags) || face->glyph->format != FT_GLYPH_FORMAT_OUTLINE)
        return 0;
    const auto &outline = face->glyph->outline;
    if (outline.n_points <= 0)
        return 0;
    uint64_t key = 14695981039346656037ULL;
    auto mix = [&](FT_Pos value) { key = (key ^ static_cast<uint64_t>(value)) * 1099511628211ULL; };
    mix(face->glyph->metrics.horiAdvance);
    mix(outline.n_points);
    // Fixed work per lookup, even for a highly detailed outline.
    for (int i : { 0, outline.n_points / 2, outline.n_points - 1 }) {
        mix(outline.points[i].x);
        mix(outline.points[i].y);
    }
    return key;
}

constexpr Unicode ambiguous = 0xffffffff;

}

std::vector<Unicode> RecoveringTextOutputDev::recover(const GfxFont &font)
{
    Ref ref;
    if (!font.getEmbeddedFontID(&ref) || bytesLeft == 0 || glyphsLeft == 0)
        return {};
    int faceIndex = 0;
    SysFontType type;
    auto path = globalParams->findSystemFontFile(font, &type, &faceIndex);
    if (!path)
        return {};
    std::vector<unsigned char> bytes; // Must outlive the FreeType memory face.
    Faces f;
    if (FT_Init_FreeType(&f.library) || FT_New_Face(f.library, path->c_str(), faceIndex, &f.installed))
        return {};
    const char *name = FT_Get_Postscript_Name(f.installed);
    if (!name || font.getNameWithoutSubsetTag() != name || FT_Select_Charmap(f.installed, FT_ENCODING_UNICODE))
        return {};

    const XRef::RepairGuard guard(recoveryXRef);
    Object stream = recoveryXRef->fetch(ref);
    if (!stream.isStream() || !stream.getStream()->rewind())
        return {};
    std::array<unsigned char, 8192> chunk;
    const size_t limit = std::min<size_t>(bytesLeft, 8 * 1024 * 1024);
    while (bytes.size() < limit) {
        int n = stream.getStream()->doGetChars(static_cast<int>(std::min(chunk.size(), limit - bytes.size())), chunk.data());
        if (n <= 0)
            break;
        bytes.insert(bytes.end(), chunk.begin(), chunk.begin() + n);
    }
    bool complete = stream.getStream()->lookChar() == EOF;
    stream.getStream()->close();
    bytesLeft -= bytes.size();
    if (!complete || bytes.empty() || FT_New_Memory_Face(f.library, bytes.data(), bytes.size(), 0, &f.embedded))
        return {};
    if (f.embedded->units_per_EM != f.installed->units_per_EM || f.embedded->num_glyphs > 65536 || f.embedded->num_glyphs < 2) {
        return {};
    }
    std::vector<Unicode> mapping(f.embedded->num_glyphs);
    const bool simple = !font.isCIDFont();
    if (simple && FT_Select_Charmap(f.embedded, FT_ENCODING_MS_SYMBOL))
        return {};
    // Compact subsets may assign entirely new GIDs. Only index uniquely
    // identifiable outlines and never infer a space from an empty outline.
    std::map<uint64_t, unsigned> subset;
    std::vector<Unicode> renumbered;
    if (font.isSubset() && mapping.size() <= 1024) {
        renumbered.resize(mapping.size());
        for (unsigned i = 1; i < mapping.size(); ++i) {
            if (const auto key = outlineKey(f.embedded, i)) {
                auto [it, inserted] = subset.emplace(key, i);
                if (!inserted)
                    it->second = 0;
            }
        }
    }
    FT_UInt gid;
    unsigned matches = 0;
    for (FT_ULong c = FT_Get_First_Char(f.installed, &gid); gid; c = FT_Get_Next_Char(f.installed, c, &gid)) {
        if (glyphsLeft == 0) {
            return {};
        }
        --glyphsLeft;
        if (c < 32 || c > 0x10ffff)
            continue;
        if (!subset.empty()) {
            const auto it = subset.find(outlineKey(f.installed, gid));
            if (it != subset.end() && it->second && sameGlyph(f, it->second, gid, c)) {
                auto &value = renumbered[it->second];
                value = value ? ambiguous : c;
            }
        }
        if (gid >= mapping.size())
            continue;
        if (mapping[gid]) {
            mapping[gid] = ambiguous; // ambiguous Unicode alias
        } else if (sameGlyph(f, gid, gid, c)) {
            mapping[gid] = c;
            if (c != ' ')
                ++matches;
        }
    }
    for (size_t i = 0; i < renumbered.size(); ++i) {
        if (!mapping[i] && renumbered[i] && renumbered[i] != ambiguous) {
            mapping[i] = renumbered[i];
            ++matches;
        }
    }
    if (matches < 3)
        return {};
    if (simple) {
        std::vector<Unicode> codes(256);
        for (unsigned code = 0; code < codes.size(); ++code) {
            unsigned g = FT_Get_Char_Index(f.embedded, code);
            if (!g)
                g = FT_Get_Char_Index(f.embedded, 0xf000 + code);
            if (g && g < mapping.size())
                codes[code] = mapping[g];
        }
        return codes;
    }
    return mapping;
}

void RecoveringTextOutputDev::drawChar(GfxState *state, double x, double y, double dx, double dy, double ox, double oy, CharCode c, int n, const Unicode *u, int len)
{
    const auto &font = state->getFont();
    if (recoveryXRef && font && !font->hasToUnicodeCMap()) {
        const GfxCIDFont *cid = nullptr;
        if (font->getType() == fontCIDType2 && (font->getEncodingName() == "Identity-H" || font->getEncodingName() == "Identity-V")) {
            const auto &candidate = static_cast<const GfxCIDFont &>(*font);
            if (candidate.getCollection() && candidate.getCollection()->toStr() == "Adobe-Identity")
                cid = &candidate;
        }
        const bool simple = font->getType() == fontTrueType && font->isSymbolic() && font->isSubset() && !static_cast<const Gfx8BitFont &>(*font).getHasEncoding();
        if (cid || simple) {
            auto it = maps.find(*font->getID());
            if (it == maps.end() && maps.size() < 64)
                it = maps.emplace(*font->getID(), recover(*font)).first;
            size_t index = c;
            if (cid && !cid->getCIDToGID().empty()) {
                const auto &cidMap = cid->getCIDToGID();
                index = c < cidMap.size() ? cidMap[c] : 0;
            }
            if (it != maps.end() && index < it->second.size()) {
                const auto &mapped = it->second[index];
                if (mapped && mapped <= 0x10ffff) {
                    u = &mapped;
                    len = 1;
                }
            }
        }
    }
    TextOutputDev::drawChar(state, x, y, dx, dy, ox, oy, c, n, u, len);
}
