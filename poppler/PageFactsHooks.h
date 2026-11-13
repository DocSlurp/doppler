//========================================================================
//
// PageFactsHooks.h
//
// DocSlurp: what Gfx tells a device about images (dictionary reads only) and the operator
// tally a device can ask Gfx to keep. Both feed page facts (PageFacts.h); a device that does
// not ask for them costs one null check per operator.
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#ifndef PAGEFACTSHOOKS_H
#define PAGEFACTSHOOKS_H

#include <array>
#include <cstddef>
#include <cstdint>
#include <unordered_map>

#include "poppler_private_export.h"
#include "Object.h"

class XRef;

// The image codec: the last filter of /Filter (the one that decodes to samples). ASCII
// filters alone count as None.
enum class ImageFilter : uint8_t
{
    None,
    DCT,
    JPX,
    JBIG2,
    CCITT,
    Flate,
    LZW,
    RunLength,
    Other
};

// The colour space family; ICC-based spaces by their component count.
enum class ImageColor : uint8_t
{
    None,
    Gray,
    RGB,
    CMYK,
    Indexed,
    Lab,
    ICC1,
    ICC3,
    ICC4,
    Separation,
    DeviceN,
    Other
};

// How an image's pixels can be taken out of the PDF without rendering the page:
//  Jpeg     the stream is a JPEG to copy unchanged (sole DCT filter, 8 bits, gray or RGB, no
//           /Decode, no /DecodeParms that could set /ColorTransform, no mask). Only the
//           dictionary says so: a copier must still check the JPEG itself (SOF baseline, its
//           component count against the colour space, its size against /Width, /Height);
//  Bilevel  1-bit samples (CCITT, JBIG2 or gray with an integer /BitsPerComponent 1, or a
//           stencil mask, /ImageMask true), no /Decode (an inverted map) and no soft mask;
//  Decode   another single image whose samples can be decoded at native size;
//  Render   anything else (inline images, masked or soft-masked images, an XObject whose data
//           is an external file, /F, and any image Gfx would refuse to draw: a non-boolean
//           /ImageMask, a missing, real or out-of-range /BitsPerComponent, ...): render the region.
enum class ImageCopy : uint8_t
{
    Render,
    Jpeg,
    Bilevel,
    Decode
};

// Facts read from an image dictionary, never from its data.
struct ImageDictInfo
{
    ImageFilter filter = ImageFilter::None;
    uint8_t filters = 0; // entries in /Filter, capped at 255
    uint8_t bpc = 0; // /BitsPerComponent, 0 when absent or not 1..32
    ImageColor color = ImageColor::None;
    bool imageMask = false; // /ImageMask true (a stencil)
    bool softMask = false; // /SMask stream or /SMaskInData > 0
    bool mask = false; // /Mask (colour key array or stencil stream)
    bool decode = false; // /Decode present
    bool colorTransform = false; // /DecodeParms present and possibly setting /ColorTransform
    bool external = false; // an XObject with /F, /FFilter or /FDecodeParms (data in another file)
    bool inlineImg = false;
    bool calibrated = false; // CalGray or CalRGB: their gamma and matrix change what the samples show
    bool defaultSpaces = false; // the resources define DefaultGray, DefaultRGB or DefaultCMYK, which replace Device spaces
    bool deviceSpace = false; // /ColorSpace is DeviceGray, DeviceRGB or DeviceCMYK by full name, or /Indexed directly over one
    bool drawable = false; // Gfx::doImage would draw it, as far as the dictionary shows (none of its early returns)
    ImageCopy copy = ImageCopy::Render;
};

POPPLER_PRIVATE_EXPORT const char *imageFilterName(ImageFilter f);
POPPLER_PRIVATE_EXPORT const char *imageColorName(ImageColor c);
POPPLER_PRIVATE_EXPORT const char *imageCopyName(ImageCopy c);
// The copy class of an image with these dictionary facts (see ImageCopy).
POPPLER_PRIVATE_EXPORT ImageCopy classifyImageCopy(const ImageDictInfo &info);

// A document's image dictionary facts, for a device that wants image geometry: Gfx reads an
// image XObject's dictionary once per document (by its reference), not once per draw, and
// classifies colour spaces (and ICC profiles' /N) reached by reference once each. A colour space
// a document draws many images with may be a large indirect object (an /Indexed lookup string);
// it is parsed once. Each map holds at most maxEntries; at most maxFetches objects are fetched
// for colour spaces over the document, after which a colour space not yet seen counts as Other;
// refusals() counts the lookups refused so (a caller compares it before and after a lookup to
// learn whether that lookup was limited), and a result a refusal shaped is never kept.
// Everything is dropped when the cross-reference table changes.
class POPPLER_PRIVATE_EXPORT ImageInfoCache
{
public:
    struct Space
    {
        ImageColor color = ImageColor::Other;
        bool calibrated = false; // CalGray or CalRGB
        // DeviceGray, DeviceRGB or DeviceCMYK by its full name, or [/Indexed base ...] with such
        // a base given directly (a base by reference does not count)
        bool device = false;
    };
    struct Image
    {
        int width = 0, height = 0; // < 1: not an image Gfx reports
        // what the dictionary alone decides (read without resources, so drawable is
        // conservative); defaultSpaces and copy are not meaningful here: Gfx sets them from the
        // resources at each draw. Never consulted by the scan-copy audit.
        ImageDictInfo info;
        bool limited = false; // a colour-space lookup for this image was refused (fetch budget)
    };
    static constexpr size_t maxEntries = 1u << 16;
    static constexpr size_t maxFetches = 1u << 16;

    // The facts stored for image <ref> of <xref>, or nullptr.
    const Image *find(Ref ref, XRef *xref);
    void store(Ref ref, XRef *xref, const Image &image);
    // The colour space family of <cs> (a /ColorSpace value, unresolved: a reference is
    // resolved here, once per document). A null value is None. Space adds whether it is
    // calibrated (CalGray or CalRGB, which read as Gray and RGB) and a plain device space.
    Space colorSpace(const Object &cs, XRef *xref);
    // Lookups refused because the fetch budget ran out (their colour space reported as Other).
    size_t refusals() const { return refused; }

private:
    void sync(XRef *xref);
    bool takeFetch();
    Space classify(const Object &cs, XRef *xref);
    Space familyOf(const Object &family, XRef *xref);
    ImageColor iccOf(const Object &profile, XRef *xref);

    XRef *docXref = nullptr;
    unsigned int generation = 0;
    std::unordered_map<Ref, Image> images;
    std::unordered_map<Ref, Space> spaces; // a colour space by reference
    std::unordered_map<Ref, Space> families; // a family name by reference
    std::unordered_map<Ref, ImageColor> profiles; // an ICC profile stream, by its /N
    size_t fetchesLeft = maxFetches;
    size_t refused = 0;
};

// Operators Gfx executed on a page, by their index in Gfx's operator table (Gfx::opName).
// "visible" while optional content is shown, "hidden" while it is not; "inLayer" the visible
// ones inside a shown optional-content section (BDC /OC). Counts saturate.
struct GfxOpTally
{
    static constexpr int maxOps = 80;
    std::array<uint64_t, maxOps> visible {};
    std::array<uint64_t, maxOps> hidden {};
    std::array<uint64_t, maxOps> inLayer {};
    uint64_t tilingPatternPaints = 0; // fills and strokes with a tiling pattern
    uint64_t shadingPatternPaints = 0; // fills and strokes with a shading pattern
    uint64_t layerXObjectsShown = 0; // XObjects with /OC that are shown
    uint64_t layerXObjectsHidden = 0; // XObjects with /OC that are hidden

    void reset() { *this = GfxOpTally(); }
};

#endif
