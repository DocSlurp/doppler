// SPDX-License-Identifier: GPL-2.0-or-later
#include "Type3FontRecovery.h"
#include "config.h"
#include "ContentReadBudget.h"
#include "GfxFont.h"
#include "Lexer.h"
#include "XRef.h"
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OUTLINE_H
#if WITH_FONTCONFIGURATION_FONTCONFIG
#    include <fontconfig/fontconfig.h>
#endif
#include <array>
#include <charconv>
#include <cmath>
#include <map>
#include <set>
#include <string_view>
#include <vector>

namespace {
struct Point
{
    char op;
    double x, y;
};
struct Path
{
    std::vector<Point> points;
    double advance = 0;
};

// Accept only a single filled, closed path with a diagonal scale. Unknown
// operators (including images, strokes and clipping) invalidate the match.
Path readPath(Gfx8BitFont &font, unsigned code, XRef *xref, ContentReadBudget &budget)
{
    const auto &m = font.getFontMatrix();
    if (m[1] || m[2] || m[4] || m[5] || m[0] <= 0 || !m[3])
        return {};
    Object proc = font.getCharProc(code);
    if (!proc.isStream())
        return {};
    budget.pageRemaining = 65536;
    budget.truncated = false;
    Lexer lexer(xref, &proc, &budget);
    Path result;
    double sx = m[0], sy = m[3];
    std::array<double, 6> args;
    size_t count = 0, start = 0;
    bool open = false, filled = false, metrics = false;
    while (result.points.size() < 4096) {
        Object token = lexer.getObj();
        if (token.isEOF())
            break;
        if (filled)
            return {};
        if (token.isNum() && count < args.size()) {
            args[count++] = token.getNum();
            continue;
        }
        if (!token.isCmd())
            return {};
        std::string_view op(token.getCmd());
        // Filling implicitly closes the last subpath, including before a new move.
        if (open && (op == "m" || op == "h" || op == "f")) {
            const auto first = result.points[start];
            if (result.points.back().x != first.x || result.points.back().y != first.y)
                result.points.push_back({ 'l', first.x, first.y });
            open = false;
            if (op == "h" && !count)
                continue;
        }
        if (op == "d1" && count == 6 && !metrics && !args[1] && result.points.empty()) {
            metrics = true;
            result.advance = args[0] * m[0];
        } else if (op == "cm" && metrics && count == 6 && !args[1] && !args[2] && !args[4] && !args[5] && result.points.empty()) {
            sx *= args[0];
            sy *= args[3];
        } else if (metrics && ((op == "m" && !open && count == 2) || (op == "l" && open && count == 2) || (op == "c" && open && count == 6))) {
            if (op == "m") {
                start = result.points.size();
                open = true;
            }
            for (size_t i = 0; i < count; i += 2)
                result.points.push_back({ op[0], args[i] * sx, args[i + 1] * sy });
        } else if (op == "f" && !open && !count && !result.points.empty()) {
            filled = true;
        } else {
            return {};
        }
        count = 0;
    }
    return filled && !budget.truncated && !count ? result : Path {};
}

struct Comparison
{
    const Path &path;
    double units;
    size_t i = 0;
    int point(char op, const FT_Vector *v)
    {
        if (i >= path.points.size())
            return 1;
        const auto &p = path.points[i++];
        // Only absorb floating-point error in the font and graphics matrices.
        return !(p.op == op && std::abs(p.x * units - v->x) < 0.00001 && std::abs(p.y * units - v->y) < 0.00001);
    }
};

bool matches(FT_Face face, unsigned gid, const Path &path)
{
    if (gid >= static_cast<unsigned long>(face->num_glyphs) || FT_Load_Glyph(face, gid, FT_LOAD_NO_SCALE | FT_LOAD_NO_BITMAP | FT_LOAD_NO_HINTING))
        return false;
    if (face->glyph->format != FT_GLYPH_FORMAT_OUTLINE || std::abs(face->glyph->metrics.horiAdvance - path.advance * face->units_per_EM) >= 0.00001)
        return false;
    static const FT_Outline_Funcs funcs = { [](const FT_Vector *p, void *s) { return static_cast<Comparison *>(s)->point('m', p); },
                                            [](const FT_Vector *p, void *s) { return static_cast<Comparison *>(s)->point('l', p); },
                                            [](const FT_Vector *, const FT_Vector *, void *) { return 1; },
                                            [](const FT_Vector *a, const FT_Vector *b, const FT_Vector *c, void *s) -> int {
                                                auto &cmp = *static_cast<Comparison *>(s);
                                                return cmp.point('c', a) || cmp.point('c', b) || cmp.point('c', c);
                                            },
                                            0,
                                            0 };
    Comparison cmp { path, static_cast<double>(face->units_per_EM) };
    return FT_Outline_Decompose(&face->glyph->outline, &funcs, &cmp) == 0 && cmp.i == path.points.size();
}
}

struct Type3FontRecovery::State
{
    FT_Library library = nullptr;
    std::map<FT_Face, std::vector<Unicode>> faces;
    std::map<std::pair<Ref, unsigned>, Unicode> cache;
    ContentReadBudget budget;

    State()
    {
        budget.runRemaining = 8 << 20;
#if WITH_FONTCONFIGURATION_FONTCONFIG
        if (FT_Init_FreeType(&library))
            return;
        std::set<std::pair<std::string, int>> seen;
        for (const auto *family : { "sans-serif", "serif" }) {
            for (int weight : { FC_WEIGHT_REGULAR, FC_WEIGHT_BOLD }) {
                FcPattern *pattern = FcPatternCreate();
                if (!pattern)
                    continue;
                FcPatternAddString(pattern, FC_FAMILY, reinterpret_cast<const FcChar8 *>(family));
                FcPatternAddInteger(pattern, FC_WEIGHT, weight);
                FcConfigSubstitute(nullptr, pattern, FcMatchPattern);
                FcDefaultSubstitute(pattern);
                FcResult res;
                FcFontSet *set = FcFontSort(nullptr, pattern, FcTrue, nullptr, &res);
                FcPatternDestroy(pattern);
                if (!set)
                    continue;
                for (int i = 0; i < std::min(8, set->nfont); ++i) {
                    FcChar8 *file;
                    int index;
                    FT_Face face;
                    if (FcPatternGetString(set->fonts[i], FC_FILE, 0, &file) == FcResultMatch && FcPatternGetInteger(set->fonts[i], FC_INDEX, 0, &index) == FcResultMatch && seen.emplace(reinterpret_cast<const char *>(file), index).second
                        && !FT_New_Face(library, reinterpret_cast<const char *>(file), index, &face)) {
                        if (face->num_glyphs > 0 && face->num_glyphs <= 65536 && !FT_Select_Charmap(face, FT_ENCODING_UNICODE))
                            faces.try_emplace(face);
                        else
                            FT_Done_Face(face);
                    }
                }
                FcFontSetDestroy(set);
            }
        }
#endif
    }

    ~State()
    {
        for (auto &entry : faces)
            FT_Done_Face(entry.first);
        if (library)
            FT_Done_FreeType(library);
    }
};

Type3FontRecovery::Type3FontRecovery() = default;
Type3FontRecovery::~Type3FontRecovery() = default;

Unicode Type3FontRecovery::lookup(Gfx8BitFont &font, unsigned code, XRef *xref)
{
    const char *name = code < 256 ? font.getCharName(code) : nullptr;
    if (!name || *name != 'g')
        return 0;
    std::string_view hex(name + 1);
    unsigned gid = 0;
    auto parsed = std::from_chars(hex.data(), hex.data() + hex.size(), gid, 16);
    if (parsed.ec != std::errc() || parsed.ptr != hex.data() + hex.size() || !gid || gid > 65535)
        return 0;
    if (!state)
        state = std::make_unique<State>();
    auto key = std::make_pair(*font.getID(), code);
    if (auto it = state->cache.find(key); it != state->cache.end())
        return it->second;
    if (state->faces.empty() || !state->budget.runRemaining || state->cache.size() >= 4096)
        return 0;
    Unicode &value = state->cache[key];
    const XRef::RepairGuard guard(xref);
    const auto path = readPath(font, code, xref, state->budget);
    if (path.points.empty())
        return 0;
    for (auto &[face, reverse] : state->faces) {
        if (!matches(face, gid, path))
            continue;
        if (reverse.empty()) {
            reverse.resize(face->num_glyphs);
            FT_UInt slot;
            for (FT_ULong c = FT_Get_First_Char(face, &slot); slot; c = FT_Get_Next_Char(face, c, &slot)) {
                if (c >= 32 && c <= 0x10ffff && slot < reverse.size())
                    reverse[slot] = reverse[slot] ? 0xffffffff : c;
            }
        }
        if (!reverse[gid] || reverse[gid] > 0x10ffff || (value && value != reverse[gid]))
            return value = 0;
        value = reverse[gid];
    }
    return value;
}
