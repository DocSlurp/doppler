// SPDX-License-Identifier: GPL-2.0-or-later
#include "FontRecovery.h"
#include "GfxFont.h"
#include "GfxState.h"
#include "GlobalParams.h"
#include "XRef.h"
#include "UnicodeTypeTable.h"
#include "goo/gmem.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_CID_H
#include <charconv>
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
uint64_t outlineKey(FT_Face face, unsigned gid, bool includeEmpty = false)
{
    if (FT_Load_Glyph(face, gid, loadFlags) || face->glyph->format != FT_GLYPH_FORMAT_OUTLINE)
        return 0;
    const auto &outline = face->glyph->outline;
    if (outline.n_points <= 0 && !includeEmpty)
        return 0;
    uint64_t key = 14695981039346656037ULL;
    auto mix = [&](FT_Pos value) { key = (key ^ static_cast<uint64_t>(value)) * 1099511628211ULL; };
    mix(face->glyph->metrics.horiAdvance);
    mix(outline.n_points);
    if (!outline.n_points)
        return key;
    // Fixed work per lookup, even for a highly detailed outline.
    for (int i : { 0, outline.n_points / 2, outline.n_points - 1 }) {
        mix(outline.points[i].x);
        mix(outline.points[i].y);
    }
    return key;
}

// Read the embedded post table using Adobe glyph names and Unicode scalar names.
Unicode namedCharacter(FT_Face embedded, unsigned gid)
{
    std::array<char, 128> buffer {};
    if (FT_Get_Glyph_Name(embedded, gid, buffer.data(), buffer.size()))
        return 0;
    std::string name(buffer.data());
    if (auto suffix = name.find('.'); suffix != std::string::npos)
        name.resize(suffix);
    Unicode u = globalParams->mapNameToUnicodeText(name.c_str());
    if (!u) {
        std::string_view hex(name);
        if (hex.starts_with("uni") && hex.size() == 7)
            hex.remove_prefix(3);
        else if (hex.starts_with("u") && hex.size() >= 5 && hex.size() <= 7)
            hex.remove_prefix(1);
        else
            return 0;
        auto result = std::from_chars(hex.data(), hex.data() + hex.size(), u, 16);
        if (result.ec != std::errc() || result.ptr != hex.data() + hex.size())
            return 0;
    }
    return u >= 32 && u <= 0x10ffff && !(u >= 0xd800 && u <= 0xdfff) ? u : 0;
}

// A CJK compatibility character and its unified ideograph may share a CID.
Unicode unifiedIdeograph(Unicode u)
{
    if (!(u >= 0x2e80 && u <= 0x2fdf) && !(u >= 0xf900 && u <= 0xfaff) && !(u >= 0x2f800 && u <= 0x2fa1f))
        return u;
    int count = 0;
    Unicode *normalized = unicodeNormalizeNFKC(&u, 1, &count, nullptr);
    if (count == 1)
        u = normalized[0];
    gfree(normalized);
    return u;
}

// Only resolve cross-script lookalikes when independently identified letters
// give a consistent script. Unknown non-ASCII ranges veto this inference.
enum class Alphabet
{
    Common,
    Latin,
    Greek,
    Cyrillic,
    Other
};
Alphabet alphabet(Unicode c)
{
    if ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= 0xc0 && c <= 0x24f && c != 0xd7 && c != 0xf7))
        return Alphabet::Latin;
    if (c >= 0x391 && c <= 0x3ff)
        return Alphabet::Greek;
    if (c >= 0x400 && c <= 0x481)
        return Alphabet::Cyrillic;
    if (c < 0xc0 || (c >= 0x2000 && c <= 0x206f))
        return Alphabet::Common;
    if (c >= 0xfb00 && c <= 0xfb06)
        return Alphabet::Latin;
    return Alphabet::Other;
}

constexpr Unicode ambiguous = 0xffffffff;

}

std::vector<Unicode> RecoveringTextOutputDev::recover(const GfxFont &font)
{
    if (font.getType() == fontType1C) {
        const auto &encoding = static_cast<const Gfx8BitFont &>(font).getEncoding();
        if (std::none_of(encoding.begin(), encoding.end(), [](const char *glyph) { return glyph && std::string_view(glyph).starts_with("cid"); }))
            return {};
    }
    Ref ref;
    if (!font.getEmbeddedFontID(&ref) || bytesLeft == 0)
        return {};
    std::vector<unsigned char> bytes; // Must outlive the FreeType memory face.
    Faces f;
    if (FT_Init_FreeType(&f.library))
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
    if (f.embedded->num_glyphs > 65536 || f.embedded->num_glyphs < 2)
        return {};
    std::vector<Unicode> named;
    if (font.getType() == fontCIDType2 && font.isSubset() && f.embedded->num_glyphs <= 1024 && FT_HAS_GLYPH_NAMES(f.embedded)) {
        named.resize(f.embedded->num_glyphs);
        for (unsigned i = 1; i < named.size(); ++i)
            named[i] = namedCharacter(f.embedded, i);
        if (std::all_of(named.begin() + 1, named.end(), [](Unicode u) { return u != 0; }))
            return named;
    }
    int faceIndex = 0;
    SysFontType type;
    auto path = globalParams->findSystemFontFile(font, &type, &faceIndex);
    if (!path || FT_New_Face(f.library, path->c_str(), faceIndex, &f.installed))
        return named;
    const char *name = FT_Get_Postscript_Name(f.installed);
    if (!name || font.getNameWithoutSubsetTag() != name || FT_Select_Charmap(f.installed, FT_ENCODING_UNICODE) || f.embedded->units_per_EM != f.installed->units_per_EM)
        return named;
    std::vector<Unicode> mapping(f.embedded->num_glyphs);
    if (font.getType() == fontType1C) {
        // Simple CFF subsets can retain original CIDs as PostScript glyph names.
        // A byte selects a name via the PDF encoding, not via its numeric value.
        const auto &simple = static_cast<const Gfx8BitFont &>(font);
        if (!FT_HAS_GLYPH_NAMES(f.embedded) || mapping.size() > 256)
            return {};
        const auto key = std::make_pair(*path, faceIndex);
        auto found = cffCharacters.find(key);
        if (found == cffCharacters.end()) {
            FT_Bool keyed = false;
            if (FT_Get_CID_Is_Internally_CID_Keyed(f.installed, &keyed) || !keyed)
                return {};
            std::map<unsigned, Candidate> index;
            FT_UInt gid;
            for (FT_ULong c = FT_Get_First_Char(f.installed, &gid); gid; c = FT_Get_Next_Char(f.installed, c, &gid)) {
                if (!glyphsLeft)
                    return {};
                --glyphsLeft;
                FT_UInt cid;
                if (c < 32 || c > 0x10ffff || FT_Get_CID_From_Glyph_Index(f.installed, gid, &cid))
                    continue;
                auto [it, inserted] = index.emplace(cid, Candidate { gid, static_cast<Unicode>(c) });
                if (!inserted) {
                    const Unicode unified = unifiedIdeograph(c);
                    it->second.unicode = it->second.gid == gid && unifiedIdeograph(it->second.unicode) == unified ? unified : 0;
                }
            }
            found = cffCharacters.emplace(key, std::move(index)).first;
        }
        std::vector<Unicode> codes(256);
        for (unsigned code = 0; code < codes.size(); ++code) {
            const char *glyph = simple.getCharName(code);
            if (!glyph || !std::string_view(glyph).starts_with("cid"))
                continue;
            std::string_view digits(glyph + 3);
            unsigned cid;
            const auto parsed = std::from_chars(digits.data(), digits.data() + digits.size(), cid);
            if (parsed.ec != std::errc() || parsed.ptr != digits.data() + digits.size())
                continue;
            const auto it = found->second.find(cid);
            unsigned embedded = FT_Get_Name_Index(f.embedded, glyph);
            if (embedded && it != found->second.end() && it->second.unicode && sameGlyph(f, embedded, it->second.gid, it->second.unicode))
                codes[code] = it->second.unicode;
        }
        return codes;
    }
    if (font.getType() == fontCIDType0C) {
        // For raw CID-keyed CFF, FreeType loads outlines by CID. Only accept
        // compact identity charsets here; sparse charsets need a separate map.
        if (!font.isSubset() || mapping.size() > 1024)
            return {};
        for (unsigned i = 0; i < mapping.size(); ++i) {
            FT_UInt cid = 0;
            if (FT_Get_CID_From_Glyph_Index(f.embedded, i, &cid) || cid != i)
                return {};
        }
        const auto key = std::make_pair(*path, faceIndex);
        auto found = cffOutlines.find(key);
        if (found == cffOutlines.end()) {
            std::map<uint64_t, Candidate> index;
            FT_UInt gid;
            for (FT_ULong c = FT_Get_First_Char(f.installed, &gid); gid; c = FT_Get_Next_Char(f.installed, c, &gid)) {
                if (!glyphsLeft)
                    return {};
                --glyphsLeft;
                if (c < 32 || c > 0x10ffff)
                    continue;
                if (auto hash = outlineKey(f.installed, gid)) {
                    auto [it, inserted] = index.emplace(hash, Candidate { gid, static_cast<Unicode>(c) });
                    if (!inserted)
                        it->second.unicode = 0;
                }
            }
            found = cffOutlines.emplace(key, std::move(index)).first;
        }
        for (unsigned i = 1; i < mapping.size(); ++i) {
            const auto it = found->second.find(outlineKey(f.embedded, i));
            if (it != found->second.end() && it->second.unicode && sameGlyph(f, i, it->second.gid, it->second.unicode))
                mapping[i] = it->second.unicode;
        }
        return mapping;
    }
    const bool simple = !font.isCIDFont();
    if (simple && FT_Select_Charmap(f.embedded, FT_ENCODING_MS_SYMBOL))
        return {};
    if (simple && mapping.size() <= 1024) {
        const auto key = std::make_pair(*path, faceIndex);
        auto found = symbolOutlines.find(key);
        if (found == symbolOutlines.end()) {
            std::map<uint64_t, std::vector<Candidate>> index;
            FT_UInt gid;
            for (FT_ULong c = FT_Get_First_Char(f.installed, &gid); gid; c = FT_Get_Next_Char(f.installed, c, &gid)) {
                if (!glyphsLeft)
                    return {};
                --glyphsLeft;
                if (c >= 32 && c <= 0x10ffff) {
                    if (auto hash = outlineKey(f.installed, gid, true))
                        index[hash].push_back({ gid, static_cast<Unicode>(c) });
                }
            }
            found = symbolOutlines.emplace(key, std::move(index)).first;
        }

        std::array<unsigned, 256> slots {};
        std::map<unsigned, std::vector<Unicode>> choices;
        for (unsigned code = 0; code < slots.size(); ++code) {
            unsigned gid = FT_Get_Char_Index(f.embedded, code);
            if (!gid)
                gid = FT_Get_Char_Index(f.embedded, 0xf000 + code);
            if (!gid || gid >= mapping.size())
                continue;
            slots[code] = gid;
            auto [entry, inserted] = choices.try_emplace(gid);
            if (!inserted)
                continue;
            const auto it = found->second.find(outlineKey(f.embedded, gid, true));
            if (it == found->second.end() || it->second.size() > 32)
                continue;
            for (const auto &candidate : it->second) {
                if (!comparisonsLeft)
                    return {};
                --comparisonsLeft;
                if (sameGlyph(f, gid, candidate.gid, candidate.unicode)) {
                    entry->second.push_back(candidate.unicode);
                }
            }
        }
        Alphabet script = Alphabet::Common;
        unsigned letters = 0, matches = 0;
        for (const auto &[gid, values] : choices) {
            if (values.size() != 1)
                continue;
            mapping[gid] = values.front();
            if (mapping[gid] != ' ')
                ++matches;
            auto a = alphabet(mapping[gid]);
            if (a != Alphabet::Common) {
                script = (script == Alphabet::Common || script == a) ? a : Alphabet::Other;
                ++letters;
            }
        }
        if (matches < 3)
            return {};
        if (letters >= 3 && script != Alphabet::Common && script != Alphabet::Other) {
            for (const auto &[gid, values] : choices) {
                if (values.size() < 2)
                    continue;
                Unicode selected = 0;
                for (Unicode c : values) {
                    const auto a = alphabet(c);
                    if (a == Alphabet::Other || a == Alphabet::Common) {
                        selected = 0;
                        break;
                    }
                    if (a == script) {
                        if (selected) {
                            selected = 0;
                            break;
                        }
                        selected = c;
                    }
                }
                mapping[gid] = selected;
            }
        }
        std::vector<Unicode> codes(256);
        for (unsigned code = 0; code < slots.size(); ++code)
            codes[code] = mapping[slots[code]];
        return codes;
    }
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
            return named;
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
        return named;
    for (unsigned i = 1; i < named.size(); ++i) {
        if ((!mapping[i] || mapping[i] == ambiguous) && named[i])
            mapping[i] = named[i];
    }
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
    Unicode recovered = 0;
    if (recoveryXRef && font && !font->hasToUnicodeCMap() && font->getType() == fontType3) {
        recovered = type3.lookup(static_cast<Gfx8BitFont &>(*font), c, recoveryXRef);
        if (recovered) {
            u = &recovered;
            len = 1;
        }
    } else if (recoveryXRef && font && !font->hasToUnicodeCMap()) {
        const GfxCIDFont *cid = nullptr;
        if ((font->getType() == fontCIDType2 || font->getType() == fontCIDType0C) && (font->getEncodingName() == "Identity-H" || font->getEncodingName() == "Identity-V")) {
            const auto &candidate = static_cast<const GfxCIDFont &>(*font);
            if (candidate.getCollection() && candidate.getCollection()->toStr() == "Adobe-Identity")
                cid = &candidate;
        }
        const bool simple = font->getType() == fontTrueType && font->isSymbolic() && font->isSubset() && !static_cast<const Gfx8BitFont &>(*font).getHasEncoding();
        if (cid || simple || (font->getType() == fontType1C && font->isSubset())) {
            auto it = maps.find(*font->getID());
            if (it == maps.end() && maps.size() < 1024 && mapSlotsLeft) {
                auto mapping = recover(*font);
                if (mapping.size() > mapSlotsLeft)
                    mapping.clear();
                mapSlotsLeft -= mapping.size();
                it = maps.emplace(*font->getID(), std::move(mapping)).first;
            }
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
