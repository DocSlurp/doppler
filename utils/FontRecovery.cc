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

bool sameGlyph(const Faces &f, unsigned gid, Unicode u)
{
    constexpr auto flags = FT_LOAD_NO_SCALE | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP | FT_LOAD_IGNORE_TRANSFORM;
    if (FT_Load_Glyph(f.embedded, gid, flags) || FT_Load_Glyph(f.installed, gid, flags))
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
    FT_UInt gid;
    unsigned matches = 0;
    for (FT_ULong c = FT_Get_First_Char(f.installed, &gid); gid; c = FT_Get_Next_Char(f.installed, c, &gid)) {
        if (glyphsLeft == 0) {
            matches = 0;
            break;
        }
        --glyphsLeft;
        if (gid >= mapping.size() || c < 32 || c > 0x10ffff)
            continue;
        if (mapping[gid]) {
            mapping[gid] = 0xffffffff; // ambiguous Unicode alias
        } else if (sameGlyph(f, gid, c)) {
            mapping[gid] = c;
            if (c != ' ')
                ++matches;
        }
    }
    if (matches < 3)
        return {};
    return mapping;
}

void RecoveringTextOutputDev::drawChar(GfxState *state, double x, double y, double dx, double dy, double ox, double oy, CharCode c, int n, const Unicode *u, int len)
{
    const auto &font = state->getFont();
    if (recoveryXRef && font && font->getType() == fontCIDType2 && !font->hasToUnicodeCMap() && (font->getEncodingName() == "Identity-H" || font->getEncodingName() == "Identity-V")) {
        const auto &cidFont = static_cast<const GfxCIDFont &>(*font);
        if (cidFont.getCollection() && cidFont.getCollection()->toStr() == "Adobe-Identity") {
            auto it = maps.find(*font->getID());
            if (it == maps.end() && maps.size() < 64)
                it = maps.emplace(*font->getID(), recover(*font)).first;
            const auto &cidMap = cidFont.getCIDToGID();
            const size_t gid = cidMap.empty() ? c : c < cidMap.size() ? cidMap[c] : 0;
            if (it != maps.end() && gid < it->second.size()) {
                const auto &mapped = it->second[gid];
                if (mapped && mapped <= 0x10ffff) {
                    u = &mapped;
                    len = 1;
                }
            }
        }
    }
    TextOutputDev::drawChar(state, x, y, dx, dy, ox, oy, c, n, u, len);
}
