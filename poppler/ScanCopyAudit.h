//========================================================================
//
// ScanCopyAudit.h
//
// DocSlurp: may a page scan be copied (the image XObject's own pixels) instead of rendered?
// Only when the copy shows what the page shows. Rather than estimating what might be drawn
// over the scan (a deny-list, which hostile pages kept getting past), this is an allow-list
// over the events that actually happen while Gfx runs the page: a page passes only when the
// one thing that paints on it is a single image XObject, drawn plainly, and everything else
// it runs is known not to paint or change how that image looks:
//
//  - q Q cm, path construction ended by n, a clip that is an axis-aligned rectangle (anything
//    clipped is caught when the image is checked against the clip in force), BT ET and text
//    state and positioning, marked content (BDC BMC EMC MP DP), colour, line and rendering
//    parameters, BX EX;
//  - text shown in render mode 3 (invisible), or mode 7 after the image (it only adds to a
//    clip nothing paints through), in a font that is not Type 3;
//  - gs whose ExtGState has only keys that cannot change what the page shows (LW LC LJ ML D
//    RI FL SA SM Font Type; OP and op false; OPM; CA and ca 1; BM Normal or Compatible;
//    SMask None; AIS false; TK);
//  - Do of a form XObject without a transparency group (its content is checked op by op);
//  - one image XObject that Gfx draws (counted in Gfx::doImage where a drawing device would
//    draw it, so an image Gfx refuses, a bad dictionary, never stands for the page), shown
//    (no /OC, not in hidden optional content), whose dictionary gives a copy class
//    (classifyImageCopy, which also needs it drawable) without masks, soft masks, an external
//    file, calibrated, ICC-based, Lab, separation or DeviceN colour, or Default colour spaces
//    (an indexed space only over DeviceGray, DeviceRGB or DeviceCMYK; a stencil mask only in
//    black), drawn upright, with a full fill opacity, the Normal blend mode, no transfer
//    function and no overprint, and lying wholly inside the clip in force and the CropBox
//    (give or take rounding: at most a point and less than half an image pixel on each axis).
//
// Anything else, at any depth (forms, annotation appearances, charprocs), denies the page:
// every other operator (fills, strokes, shadings, inline images, Type 3 glyph metrics),
// visible text in any mode, a second image, a page /Group, a transparency group, a soft mask,
// a non-rectangular or stroke clip, any annotation that draws an appearance or border, in
// view or in print mode (only a link or popup with no appearance and no border is let pass),
// and /OutputIntents on the catalog or the page (a viewer may show the page through their
// destination profile, which a copy of the samples does not carry). So does anything a cost
// limit left undecided, since Gfx then draws what a full evaluation might not: optional content
// taken as visible because the page's budget was spent or an expression was too large, marked
// content nested past Gfx's cap (its visibility not tracked), and an image whose colour-space
// lookup the document's ImageInfoCache refused.
// Operators the table does not know are ignored by Gfx and paint nothing.
//
// Gfx feeds it (OutputDev::getScanCopyAudit, asked once per page Gfx): op() before each
// operator runs, imageDict() and imageDrawn() from doImage (on a device that draws nothing,
// doImage runs an image XObject's dictionary through its checks without reading its data),
// annotation() when an annotation is drawn. The device feeds the rest:
// startPage(), clip() from its clip callbacks, deny() from transparency groups and soft masks.
// pdftotext (TextOutputDev) and pdfregions (its own minimal device) both run it, so a copy
// needs both to agree.
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#ifndef SCANCOPYAUDIT_H
#define SCANCOPYAUDIT_H

#include <cstdint>

#include "poppler_private_export.h"
#include "Object.h"

class AnnotBorder;
class AnnotColor;
class Dict;
class GfxResources;
class GfxState;
class ImageInfoCache;
class Page;

class POPPLER_PRIVATE_EXPORT ScanCopyAudit
{
public:
    // Why a page is not copyable; the first reason found is kept.
    enum class Reason : uint8_t
    {
        None,
        NoImage, // nothing painted
        Operator, // an operator that paints or is not on the allow-list
        Text, // text in a visible render mode, or mode 7 before the image
        Type3, // a Type 3 font shown
        ExtGState, // a gs key that may change what the page shows
        Form, // a form with a transparency group, or a PostScript or unknown XObject
        SecondImage, // more than one image drawn
        Hidden, // the image is in optional content or hidden
        ImageDict, // its dictionary: no copy class, masks, colour space
        ImagePaint, // opacity, blend mode, transfer function, overprint, stencil colour
        Orientation, // not upright
        Clip, // a clip cuts the image, or a clip that is not a rectangle
        CropBox, // the image reaches past the CropBox
        PageGroup, // the page has a transparency group
        Group, // a transparency group
        SoftMask, // a soft mask
        Annotation, // an annotation drew something
        OutputIntent, // the document (or the page) has /OutputIntents: a viewer may show the page through their profile
        Unevaluated, // a cost limit left something undecided: optional content taken as visible (its budget spent or its expression too large), marked content nested past Gfx's cap, or a colour-space lookup the ImageInfoCache refused
    };

    // A new page: forget the last one. The state's CTM is the page's base matrix.
    void startPage(Page *page, const GfxState *state);
    // Gfx is about to run operator <opIndex> (Gfx::opName) with these arguments.
    void op(int opIndex, Object args[], int numArgs, GfxState *state, GfxResources *res, bool ocState);
    // The device's clip callbacks: the clip path is state's current path.
    void clip(const GfxState *state);
    // An annotation's appearance stream (or nothing) and border are about to be drawn.
    void annotation(const Object *appearance, const AnnotBorder *border, const AnnotColor *color);
    // Gfx::doImage, before it reads anything else of an image XObject on the audit-only path:
    // false (and the page denied) when the dictionary rules a copy out, so nothing more of the
    // image needs reading.
    bool imageDict(Dict *dict, GfxResources *res);
    // Gfx::doImage is about to draw the image XObject <ref> (width x height samples) with this
    // state: called exactly where a drawing device's drawImage (or drawImageMask) would run,
    // so an image Gfx refuses to draw is never counted.
    void imageDrawn(GfxState *state, GfxResources *res, Dict *dict, const Object *ref, int width, int height);
    void deny(Reason why);
    // The document's ImageInfoCache (may be null; kept across pages), used only to classify
    // colour spaces by reference, so a large shared one is parsed once per document rather
    // than on every page. Image facts are never taken from it: each image is read where drawn.
    void setColorCache(ImageInfoCache *cache) { colorCache = cache; }

    // The page passes for the image object <image>: it drew exactly that image, once, and
    // nothing else that paints.
    bool copyable(Ref image) const { return started && reason == Reason::None && images == 1 && imageRef == image; }
    Reason why() const { return !started ? Reason::NoImage : reason != Reason::None ? reason : images == 0 ? Reason::NoImage : Reason::None; }
    static const char *reasonName(Reason r);

private:
    void extGState(GfxResources *res, const Object &name);

    bool started = false;
    Reason reason = Reason::None;
    int images = 0;
    Ref imageRef = Ref::INVALID();
    ImageInfoCache *colorCache = nullptr;
    double cropX0 = 0, cropY0 = 0, cropX1 = 0, cropY1 = 0; // the CropBox in device space
    double tolerance = 1; // a point, in device units (at most half an image pixel is used)
};

#endif
