//========================================================================
//
// ScanCopyAudit.cc
//
// DocSlurp: the allow-list behind page-scan copies (see ScanCopyAudit.h).
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#include <config.h>

#include "ScanCopyAudit.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <string>

#include "Annot.h"
#include "Gfx.h"
#include "GfxFont.h"
#include "GfxState.h"
#include "Page.h"
#include "Stream.h"
#include "PageFactsHooks.h"

namespace {

enum class OpKind : uint8_t
{
    Deny, // paints, or is not known not to
    Allow, // paints nothing and does not change how a later image looks
    Show, // shows text: allowed only invisibly
    ExtGState, // gs: allowed for some keys
    XObject, // Do: an image or a form
};

// The kind of each operator of Gfx's table, by index; names not listed below are denied, so an
// operator added to the table later is denied until it is reviewed.
const std::array<OpKind, GfxOpTally::maxOps> &opKinds()
{
    static const std::array<OpKind, GfxOpTally::maxOps> kinds = [] {
        std::array<OpKind, GfxOpTally::maxOps> k {};
        k.fill(OpKind::Deny);
        static const char *const allowed[] = { // graphics state, transformation
                                               "q", "Q", "cm", "w", "J", "j", "M", "d", "ri", "i",
                                               // path construction, ending a path without painting it, clips
                                               "m", "l", "c", "v", "y", "h", "re", "n", "W", "W*",
                                               // text objects, state and positioning
                                               "BT", "ET", "Tc", "Tw", "Tz", "TL", "Tf", "Tr", "Ts", "Td", "TD", "Tm", "T*",
                                               // marked content
                                               "BDC", "BMC", "EMC", "MP", "DP",
                                               // colour
                                               "CS", "cs", "SC", "SCN", "sc", "scn", "G", "g", "RG", "rg", "K", "k",
                                               // compatibility sections
                                               "BX", "EX"
        };
        for (int i = 0; i < Gfx::opCount() && i < GfxOpTally::maxOps; ++i) {
            const char *name = Gfx::opName(i);
            for (const char *a : allowed) {
                if (strcmp(name, a) == 0) {
                    k[i] = OpKind::Allow;
                }
            }
            if (!strcmp(name, "Tj") || !strcmp(name, "TJ") || !strcmp(name, "'") || !strcmp(name, "\"")) {
                k[i] = OpKind::Show;
            } else if (!strcmp(name, "gs")) {
                k[i] = OpKind::ExtGState;
            } else if (!strcmp(name, "Do")) {
                k[i] = OpKind::XObject;
            }
        }
        return k;
    }();
    return kinds;
}

bool isNum(const Object &o, double v)
{
    return o.isNum() && o.getNum() == v;
}

// Gfx::drawAnnot draws a border of positive width in a colour that is not transparent
bool drawsBorder(const AnnotBorder *border, const AnnotColor *color)
{
    return border && border->getWidth() > 0 && (!color || color->getSpace() != AnnotColor::colorTransparent);
}

bool isDeviceSpaceName(const Object &o)
{
    return o.isName("DeviceGray") || o.isName("DeviceRGB") || o.isName("DeviceCMYK");
}

} // namespace

const char *ScanCopyAudit::reasonName(Reason r)
{
    switch (r) {
    case Reason::None:
        return "none";
    case Reason::NoImage:
        return "no-image";
    case Reason::Operator:
        return "operator";
    case Reason::Text:
        return "text";
    case Reason::Type3:
        return "type3";
    case Reason::ExtGState:
        return "extgstate";
    case Reason::Form:
        return "form";
    case Reason::SecondImage:
        return "second-image";
    case Reason::Hidden:
        return "hidden";
    case Reason::ImageDict:
        return "image-dict";
    case Reason::ImagePaint:
        return "image-paint";
    case Reason::Orientation:
        return "orientation";
    case Reason::Clip:
        return "clip";
    case Reason::CropBox:
        return "cropbox";
    case Reason::PageGroup:
        return "page-group";
    case Reason::Group:
        return "group";
    case Reason::SoftMask:
        return "softmask";
    case Reason::Annotation:
        return "annotation";
    }
    return "unknown";
}

void ScanCopyAudit::deny(Reason why)
{
    if (reason == Reason::None) {
        reason = why;
    }
}

void ScanCopyAudit::startPage(Page *page, const GfxState *state)
{
    *this = ScanCopyAudit();
    started = true;
    if (!page || !state) {
        deny(Reason::CropBox);
        return;
    }
    if (page->getGroup()) {
        deny(Reason::PageGroup);
    }
    const std::array<double, 6> &m = state->getCTM();
    const PDFRectangle &box = page->getCropBox();
    double xs[4], ys[4];
    for (int i = 0; i < 4; ++i) {
        const double x = i & 1 ? box.x2 : box.x1, y = i & 2 ? box.y2 : box.y1;
        xs[i] = m[0] * x + m[2] * y + m[4];
        ys[i] = m[1] * x + m[3] * y + m[5];
    }
    cropX0 = std::min({ xs[0], xs[1], xs[2], xs[3] });
    cropX1 = std::max({ xs[0], xs[1], xs[2], xs[3] });
    cropY0 = std::min({ ys[0], ys[1], ys[2], ys[3] });
    cropY1 = std::max({ ys[0], ys[1], ys[2], ys[3] });
    const double hdpi = state->getHDPI() > 0 ? state->getHDPI() : 72.0;
    tolerance = hdpi / 72.0;
    if (!std::isfinite(cropX0) || !std::isfinite(cropX1) || !std::isfinite(cropY0) || !std::isfinite(cropY1) || !std::isfinite(tolerance)) {
        deny(Reason::CropBox);
    }
}

void ScanCopyAudit::op(int opIndex, Object args[], int numArgs, GfxState *state, GfxResources *res, bool /*ocState*/)
{
    if (reason != Reason::None) {
        return; // already denied: the rest of the page costs one compare per operator
    }
    const auto &kinds = opKinds();
    const OpKind kind = opIndex >= 0 && opIndex < static_cast<int>(kinds.size()) ? kinds[opIndex] : OpKind::Deny;
    switch (kind) {
    case OpKind::Allow:
        return;
    case OpKind::Deny:
        deny(Reason::Operator);
        return;
    case OpKind::Show: {
        const GfxFont *font = state->getFont().get();
        if (!font) {
            deny(Reason::Text);
            return;
        }
        if (font->getType() == fontType3) {
            deny(Reason::Type3);
            return;
        }
        // mode 3 paints nothing; mode 7 only adds to the clip, which nothing paints through
        // once the image is drawn (any later paint denies the page anyway)
        const int render = state->getRender();
        if (render == 3 || (render == 7 && images > 0)) {
            return;
        }
        deny(Reason::Text);
        return;
    }
    case OpKind::ExtGState:
        if (numArgs < 1 || !res) {
            deny(Reason::ExtGState);
            return;
        }
        extGState(res, args[0]);
        return;
    case OpKind::XObject: {
        if (numArgs < 1 || !args[0].isName() || !res) {
            deny(Reason::Form);
            return;
        }
        const std::string &name = args[0].getNameString();
        Object xobj = res->lookupXObject(name);
        if (xobj.isNull()) {
            return; // Gfx draws nothing
        }
        if (!xobj.isStream()) {
            deny(Reason::Form);
            return;
        }
        Dict *dict = xobj.getStream()->getDict();
        Object subtype = dict->lookup("Subtype");
        if (subtype.isName("Image")) {
            // counted by Gfx::doImage where it would draw it (imageDrawn): an image Gfx refuses
            // to draw (a bad dictionary) shows nothing and must not stand for the page
        } else if (subtype.isName("Form")) {
            // its content runs through op() as well; a group changes how it composites, which
            // no operator inside shows
            if (!dict->lookupNF("Group").isNull()) {
                deny(Reason::Group);
            }
        } else {
            deny(Reason::Form);
        }
        return;
    }
    }
    deny(Reason::Operator);
}

void ScanCopyAudit::extGState(GfxResources *res, const Object &name)
{
    if (!name.isName()) {
        deny(Reason::ExtGState);
        return;
    }
    Object gs = res->lookupGState(name.getNameString());
    if (!gs.isDict()) {
        deny(Reason::ExtGState);
        return;
    }
    Dict *dict = gs.getDict();
    for (int i = 0; i < dict->getLength(); ++i) {
        const std::string &keyStr = dict->getKey(i);
        const char *key = keyStr.c_str();
        // line and rendering parameters, flatness, stroke adjustment, smoothness and fonts
        // change nothing an image shows
        static const char *const harmless[] = { "Type", "LW", "LC", "LJ", "ML", "D", "RI", "FL", "SA", "SM", "Font", "OPM", "TK" };
        bool ok = false;
        for (const char *h : harmless) {
            if (strcmp(key, h) == 0) {
                ok = true;
                break;
            }
        }
        if (!ok) {
            Object v = dict->getVal(i);
            if (!strcmp(key, "OP") || !strcmp(key, "op") || !strcmp(key, "AIS")) {
                ok = v.isBool() && !v.getBool();
            } else if (!strcmp(key, "CA") || !strcmp(key, "ca")) {
                ok = isNum(v, 1.0);
            } else if (!strcmp(key, "BM")) {
                ok = v.isName("Normal") || v.isName("Compatible");
            } else if (!strcmp(key, "SMask")) {
                ok = v.isName("None");
            }
        }
        if (!ok) {
            deny(Reason::ExtGState);
            return;
        }
    }
}

bool ScanCopyAudit::imageDict(Dict *dict, GfxResources *res)
{
    if (reason != Reason::None) {
        return false;
    }
    // optional content of its own (whether shown or not, render it)
    if (!dict->lookupNF("OC").isNull()) {
        deny(Reason::Hidden);
        return false;
    }
    // the dictionary: a copy class (which Gfx can draw), nothing masked, device colour only
    const ImageDictInfo info = Gfx::readImageDictInfo(dict, false, res);
    if (!info.drawable || info.copy == ImageCopy::Render || info.external || info.softMask || info.mask || info.calibrated || info.defaultSpaces || info.inlineImg) {
        deny(Reason::ImageDict);
        return false;
    }
    switch (info.color) {
    case ImageColor::Gray:
    case ImageColor::RGB:
    case ImageColor::CMYK:
        break;
    case ImageColor::Indexed: {
        Object cs = dict->lookup("ColorSpace");
        if (!cs.isArray() || cs.arrayGetLength() < 2 || !isDeviceSpaceName(cs.arrayGet(1))) {
            deny(Reason::ImageDict);
            return false;
        }
        break;
    }
    case ImageColor::None:
        if (info.imageMask) {
            break;
        }
        deny(Reason::ImageDict);
        return false;
    default:
        deny(Reason::ImageDict);
        return false;
    }
    // CalGray and CalRGB read as Gray and RGB above, and are caught by info.calibrated; a
    // device colour space must be named as one
    if (!info.imageMask) {
        Object cs = dict->lookup("ColorSpace");
        if (!(isDeviceSpaceName(cs) || (cs.isArray() && cs.arrayGetLength() >= 1 && cs.arrayGet(0).isName("Indexed")))) {
            deny(Reason::ImageDict);
            return false;
        }
    }
    return true;
}

void ScanCopyAudit::imageDrawn(GfxState *state, GfxResources *res, Dict *dict, const Object *ref, int width, int height)
{
    if (++images > 1) {
        deny(Reason::SecondImage);
        return;
    }
    if (!ref || !ref->isRef() || width < 1 || height < 1) {
        deny(Reason::ImageDict);
        return;
    }
    imageRef = ref->getRef();
    if (!imageDict(dict, res)) {
        return;
    }
    const bool imageMask = Gfx::readImageDictInfo(dict, false, res).imageMask;

    // how it is painted
    if (state->getFillOpacity() != 1.0 || state->getBlendMode() != gfxBlendNormal || !state->getTransfer().empty() || state->getFillOverprint()) {
        deny(Reason::ImagePaint);
        return;
    }
    if (imageMask) {
        // a stencil paints the fill colour: copied, it is black
        const GfxColorSpace *fcs = state->getFillColorSpace();
        const GfxColor *fc = &state->getFillColor();
        const bool black = fcs && ((fcs->getMode() == csDeviceGray && fc->c[0] == 0) || (fcs->getMode() == csDeviceRGB && fc->c[0] == 0 && fc->c[1] == 0 && fc->c[2] == 0));
        if (!black) {
            deny(Reason::ImagePaint);
            return;
        }
    }

    // where: upright, and wholly inside the clip in force and the CropBox
    double x[4], y[4];
    state->transform(0, 0, &x[0], &y[0]);
    state->transform(1, 0, &x[1], &y[1]);
    state->transform(0, 1, &x[2], &y[2]);
    state->transform(1, 1, &x[3], &y[3]);
    for (int i = 0; i < 4; ++i) {
        if (!std::isfinite(x[i]) || !std::isfinite(y[i])) {
            deny(Reason::Orientation);
            return;
        }
    }
    const double cx = x[1] - x[0], cy = y[1] - y[0], rx = x[0] - x[2], ry = y[0] - y[2];
    auto along = [](double major, double minor) { return std::fabs(minor) <= 1e-3 * std::fabs(major); };
    if (!(cx > 0 && ry > 0 && along(cx, cy) && along(ry, rx))) {
        deny(Reason::Orientation);
        return;
    }
    const double tolX = tolerance, tolY = tolerance;
    const double ux0 = std::min({ x[0], x[1], x[2], x[3] }), ux1 = std::max({ x[0], x[1], x[2], x[3] });
    const double uy0 = std::min({ y[0], y[1], y[2], y[3] }), uy1 = std::max({ y[0], y[1], y[2], y[3] });
    double kx0, ky0, kx1, ky1;
    state->getClipBBox(&kx0, &ky0, &kx1, &ky1);
    if (!(kx0 <= ux0 + tolX && ky0 <= uy0 + tolY && kx1 >= ux1 - tolX && ky1 >= uy1 - tolY)) {
        deny(Reason::Clip);
        return;
    }
    if (!(cropX0 <= ux0 + tolX && cropY0 <= uy0 + tolY && cropX1 >= ux1 - tolX && cropY1 >= uy1 - tolY)) {
        deny(Reason::CropBox);
        return;
    }
}

void ScanCopyAudit::clip(const GfxState *state)
{
    if (reason != Reason::None) {
        return;
    }
    // an axis-aligned rectangle is exactly what the clip box says (the image is checked against
    // that box); anything else may hide part of an image inside the box
    const GfxPath *path = state->getPath();
    bool rectangular = path && path->getNumSubpaths() == 1;
    if (rectangular) {
        const GfxSubpath *sub = path->getSubpath(0);
        const int n = sub->getNumPoints();
        rectangular = n == 4 || n == 5;
        double px[5], py[5];
        for (int i = 0; rectangular && i < n; ++i) {
            rectangular = !sub->getCurve(i);
            state->transform(sub->getX(i), sub->getY(i), &px[i], &py[i]);
            rectangular = rectangular && std::isfinite(px[i]) && std::isfinite(py[i]);
        }
        if (rectangular) {
            const double bx0 = *std::min_element(px, px + n), bx1 = *std::max_element(px, px + n);
            const double by0 = *std::min_element(py, py + n), by1 = *std::max_element(py, py + n);
            const double eps = 0.01;
            int corners = 0;
            for (int i = 0; rectangular && i < n; ++i) {
                const bool left = std::fabs(px[i] - bx0) <= eps, right = std::fabs(px[i] - bx1) <= eps;
                const bool top = std::fabs(py[i] - by0) <= eps, bottom = std::fabs(py[i] - by1) <= eps;
                rectangular = (left || right) && (top || bottom);
                corners |= 1 << ((right ? 1 : 0) + (bottom ? 2 : 0));
            }
            rectangular = rectangular && corners == 15;
            for (int i = 0; rectangular && i + 1 < n; ++i) {
                rectangular = std::fabs(px[i] - px[i + 1]) <= eps || std::fabs(py[i] - py[i + 1]) <= eps;
            }
            // a closing point must return to the start
            rectangular = rectangular && (n == 4 || (std::fabs(px[4] - px[0]) <= eps && std::fabs(py[4] - py[0]) <= eps));
        }
    }
    if (!rectangular) {
        deny(Reason::Clip);
    }
}

void ScanCopyAudit::annotation(const Object *appearance, const AnnotBorder *border, const AnnotColor *color)
{
    // Gfx::drawAnnot draws the appearance stream, then the border
    if ((appearance && appearance->isStream()) || drawsBorder(border, color)) {
        deny(Reason::Annotation);
    }
}
