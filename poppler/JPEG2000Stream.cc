//========================================================================
//
// JPEG2000Stream.cc
//
// A JPX stream decoder using OpenJPEG
//
// Copyright 2008-2010, 2012, 2017-2023, 2025, 2026 Albert Astals Cid <aacid@kde.org>
// Copyright 2011 Daniel Glöckner <daniel-gl@gmx.net>
// Copyright 2014, 2016 Thomas Freitag <Thomas.Freitag@alfa.de>
// Copyright 2013, 2014 Adrian Johnson <ajohnson@redneon.com>
// Copyright 2015 Adam Reichold <adam.reichold@t-online.de>
// Copyright 2015 Jakub Wilk <jwilk@jwilk.net>
// Copyright 2022 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright 2024, 2025 Nelson Benítez León <nbenitezl@gmail.com>
// Copyright 2025, 2026 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright (C) 2025 Arnav V <arnav0872@gmail.com>
// Copyright (C) 2026 Adam Sampson <ats@offog.org>
// Copyright (C) 2026 William Bader <williambader@hotmail.com>
//
// Licensed under GPLv2 or later
//
//========================================================================

#include "config.h"
#include "JPEG2000Stream.h"
#include <openjpeg.h>
#include <cstdint>
#include <cstring>
#include <vector>

struct JPXStreamPrivate
{
    opj_image_t *image = nullptr;
    int counter = 0;
    int ccounter = 0;
    int npixels = 0;
    int ncomps = 0;
    bool inited = false;
    void init2(OPJ_CODEC_FORMAT format, const unsigned char *buf, int length, bool indexed);
    // Component count and colour space as reported by the codestream/JP2 header,
    // without decoding the image.  Returns false when the header alone does not
    // tell what a full decode would produce; callers must then decode.
    bool readHeaderOnly(const unsigned char *buf, int length, int *numcomps, OPJ_COLOR_SPACE *colorSpace);
};

static inline unsigned char adjustComp(int r, int adjust, int depth, int sgndcorr, bool indexed)
{
    if (!indexed) {
        r += sgndcorr;
        if (adjust) {
            r = (r >> adjust) + ((r >> (adjust - 1)) % 2);
        } else if (depth < 8) {
            r = r << (8 - depth);
        }
    }
    if (unlikely(r > 255)) {
        r = 255;
    }
    return r;
}

static inline int doLookChar(JPXStreamPrivate *priv)
{
    if (unlikely(priv->counter >= priv->npixels)) {
        return EOF;
    }

    return (reinterpret_cast<unsigned char *>(priv->image->comps[priv->ccounter].data))[priv->counter];
}

static inline int doGetChar(JPXStreamPrivate *priv)
{
    const int result = doLookChar(priv);
    if (++priv->ccounter == priv->ncomps) {
        priv->ccounter = 0;
        ++priv->counter;
    }
    return result;
}

JPXStream::JPXStream(std::unique_ptr<Stream> strA) : OwnedFilterStream(std::move(strA))
{
    priv = new JPXStreamPrivate;
    handleJPXtransparency = false;
}

JPXStream::~JPXStream()
{
    close();
    delete priv;
}

bool JPXStream::rewind()
{
    // close() frees the decoded image without clearing inited, so without this
    // every read after a close() would return EOF while rewind() still
    // reported success.  Decoding is expensive, so only re-init when the image
    // is actually gone; otherwise resetting the counters is enough.
    if (priv->image == nullptr) {
        priv->inited = false;
    }

    priv->counter = 0;
    priv->ccounter = 0;

    return true;
}

void JPXStream::close()
{
    if (priv->image != nullptr) {
        opj_image_destroy(priv->image);
        priv->image = nullptr;
        priv->npixels = 0;
    }
}

Goffset JPXStream::getPos()
{
    return priv->counter * priv->ncomps + priv->ccounter;
}

int JPXStream::getChars(int nChars, unsigned char *buffer)
{
    if (unlikely(priv->inited == false)) {
        init();
    }

    for (int i = 0; i < nChars; ++i) {
        const int c = doGetChar(priv);
        if (likely(c != EOF)) {
            buffer[i] = c;
        } else {
            return i;
        }
    }
    return nChars;
}

int JPXStream::getChar()
{
    if (unlikely(priv->inited == false)) {
        init();
    }

    return doGetChar(priv);
}

int JPXStream::lookChar()
{
    if (unlikely(priv->inited == false)) {
        init();
    }

    return doLookChar(priv);
}

std::optional<std::string> JPXStream::getPSFilter(int /*psLevel*/, const char * /*indent*/)
{
    return {};
}

bool JPXStream::isBinary(bool /*last*/) const
{
    return str->isBinary(true);
}

void JPXStream::getImageParams(int *bitsPerComponent, StreamColorSpaceMode *csMode, bool *hasAlpha)
{
    // Callers such as Gfx::doImage ask for the image parameters of every
    // image, including ones that are never read (pdfimages -list, invisible
    // optional content, singular matrices).  A header read is enough to answer,
    // so only decode when the header cannot tell the final component layout.
    // The full decode still happens lazily on the first read of pixel data.
    int numComps = 1;
    OPJ_COLOR_SPACE colorSpace = OPJ_CLRSPC_UNSPECIFIED;
    bool haveParams = false;
    if (priv->inited) {
        if (priv->image) {
            numComps = priv->image->numcomps;
            colorSpace = priv->image->color_space;
        }
        haveParams = true;
    } else {
        const std::vector<unsigned char> buf = str->toUnsignedChars(streamBufferSize());
        haveParams = priv->readHeaderOnly(buf.data(), static_cast<int>(buf.size()), &numComps, &colorSpace);
    }
    if (!haveParams) {
        init();
        if (priv->image) {
            numComps = priv->image->numcomps;
            colorSpace = priv->image->color_space;
        }
    }

    *bitsPerComponent = 8;
    *hasAlpha = false;
    if (colorSpace == OPJ_CLRSPC_SRGB && numComps == 4) {
        numComps = 3;
        *hasAlpha = true;
    } else if (colorSpace == OPJ_CLRSPC_SYCC && numComps == 4) {
        numComps = 3;
        *hasAlpha = true;
    } else if (numComps == 2) {
        numComps = 1;
    } else if (numComps > 4) {
        *hasAlpha = true;
        numComps = 4;
    }
    if (numComps == 3) {
        *csMode = streamCSDeviceRGB;
    } else if (numComps == 4) {
        *csMode = streamCSDeviceCMYK;
    } else {
        *csMode = streamCSDeviceGray;
    }
}

static void libopenjpeg_error_callback(const char *msg, void * /*client_data*/)
{
    error(errSyntaxError, -1, "{0:s}", msg);
}

static void libopenjpeg_warning_callback(const char *msg, void * /*client_data*/)
{
    error(errSyntaxWarning, -1, "{0:s}", msg);
}

struct JPXData
{
    const unsigned char *data;
    int size;
    OPJ_OFF_T pos;
};

constexpr int BUFFER_INITIAL_SIZE = 4096;

static OPJ_SIZE_T jpxRead_callback(void *p_buffer, OPJ_SIZE_T p_nb_bytes, void *p_user_data)
{
    auto *jpxData = static_cast<JPXData *>(p_user_data);

    if (unlikely(jpxData->size <= jpxData->pos)) {
        return static_cast<OPJ_SIZE_T>(-1); /* End of file! */
    }
    OPJ_SIZE_T len = jpxData->size - jpxData->pos;
    if (len > p_nb_bytes) {
        len = p_nb_bytes;
    }
    memcpy(p_buffer, jpxData->data + jpxData->pos, len);
    jpxData->pos += len;
    return len;
}

static OPJ_OFF_T jpxSkip_callback(OPJ_OFF_T skip, void *p_user_data)
{
    auto *jpxData = static_cast<JPXData *>(p_user_data);

    jpxData->pos += (skip > jpxData->size - jpxData->pos) ? jpxData->size - jpxData->pos : skip;
    /* Always return input value to avoid "Problem with skipping JPEG2000 box, stream error" */
    return skip;
}

static OPJ_BOOL jpxSeek_callback(OPJ_OFF_T seek_pos, void *p_user_data)
{
    auto *jpxData = static_cast<JPXData *>(p_user_data);

    if (seek_pos > jpxData->size) {
        return OPJ_FALSE;
    }
    jpxData->pos = seek_pos;
    return OPJ_TRUE;
}

static uint32_t jpxBE32(const unsigned char *p)
{
    return (static_cast<uint32_t>(p[0]) << 24) | (static_cast<uint32_t>(p[1]) << 16) | (static_cast<uint32_t>(p[2]) << 8) | p[3];
}

enum class JP2BoxScan
{
    NoPalette, // well-formed header without a palette: the codestream component count is final
    Palette, // a pclr box is present: decoding expands the palette into more components
    Unknown // anything unexpected: let the full decoder sort it out
};

// Walk the boxes in [pos, end) looking for a palette ("pclr") box. When
// descend is true, a jp2h super box is searched recursively. Stops at the
// codestream box, which must come after the JP2 header box.
static JP2BoxScan scanJP2Boxes(const unsigned char *buf, size_t pos, size_t end, bool descend, bool *sawHeader)
{
    while (pos + 8 <= end) {
        uint64_t boxLen = jpxBE32(buf + pos);
        const uint32_t boxType = jpxBE32(buf + pos + 4);
        size_t hdrLen = 8;
        if (boxLen == 1) {
            if (pos + 16 > end) {
                return JP2BoxScan::Unknown;
            }
            boxLen = (static_cast<uint64_t>(jpxBE32(buf + pos + 8)) << 32) | jpxBE32(buf + pos + 12);
            hdrLen = 16;
        } else if (boxLen == 0) {
            boxLen = end - pos;
        }
        if (boxLen < hdrLen || boxLen > end - pos) {
            return JP2BoxScan::Unknown;
        }
        if (boxType == 0x70636c72) { // 'pclr'
            return JP2BoxScan::Palette;
        }
        if (boxType == 0x6a703268) { // 'jp2h'
            if (!descend) {
                return JP2BoxScan::Unknown;
            }
            *sawHeader = true;
            const JP2BoxScan r = scanJP2Boxes(buf, pos + hdrLen, pos + boxLen, false, sawHeader);
            if (r != JP2BoxScan::NoPalette) {
                return r;
            }
        } else if (boxType == 0x6a703263) { // 'jp2c'
            return *sawHeader ? JP2BoxScan::NoPalette : JP2BoxScan::Unknown;
        }
        pos += boxLen;
    }
    // Inside jp2h, running out of boxes means there is no palette. At the top
    // level the codestream box must have been found first.
    return descend ? JP2BoxScan::Unknown : JP2BoxScan::NoPalette;
}

bool JPXStreamPrivate::readHeaderOnly(const unsigned char *buf, int length, int *numcomps, OPJ_COLOR_SPACE *colorSpace)
{
    static const unsigned char jp2Signature[12] = { 0x00, 0x00, 0x00, 0x0c, 0x6a, 0x50, 0x20, 0x20, 0x0d, 0x0a, 0x87, 0x0a };

    OPJ_CODEC_FORMAT format;
    if (length >= 12 && memcmp(buf, jp2Signature, 12) == 0) {
        bool sawHeader = false;
        if (scanJP2Boxes(buf, 12, length, true, &sawHeader) != JP2BoxScan::NoPalette) {
            return false;
        }
        format = OPJ_CODEC_JP2;
    } else if (length >= 4 && buf[0] == 0xff && buf[1] == 0x4f && buf[2] == 0xff && buf[3] == 0x51) {
        format = OPJ_CODEC_J2K;
    } else {
        return false;
    }

    JPXData jpxData;
    jpxData.data = buf;
    jpxData.pos = 0;
    jpxData.size = length;

    opj_stream_t *stream = opj_stream_default_create(OPJ_TRUE);
    opj_stream_set_user_data(stream, &jpxData, nullptr);
    opj_stream_set_read_function(stream, jpxRead_callback);
    opj_stream_set_skip_function(stream, jpxSkip_callback);
    opj_stream_set_seek_function(stream, jpxSeek_callback);
    opj_stream_set_user_data_length(stream, length);

    bool ok = false;
    opj_codec_t *decoder = opj_create_decompress(format);
    if (decoder != nullptr) {
        // Stay silent: if the header is unusable the caller falls back to the
        // full decode, which reports the problem.
        opj_dparameters_t parameters;
        opj_set_default_decoder_parameters(&parameters);
        opj_image_t *hdrImage = nullptr;
        if (opj_setup_decoder(decoder, &parameters) && opj_read_header(stream, decoder, &hdrImage) && hdrImage != nullptr) {
            *numcomps = hdrImage->numcomps;
            *colorSpace = hdrImage->color_space;
            ok = true;
        }
        if (hdrImage != nullptr) {
            opj_image_destroy(hdrImage);
        }
        opj_destroy_codec(decoder);
    }
    opj_stream_destroy(stream);
    return ok;
}

int JPXStream::streamBufferSize()
{
    if (getDict()) {
        const Object oLen = getDict()->lookup("Length");
        if (oLen.isInt() && oLen.getInt() > 0) {
            return oLen.getInt();
        }
    }
    return BUFFER_INITIAL_SIZE;
}

void JPXStream::init()
{
    Object cspace, smaskInDataObj;
    if (getDict()) {
        cspace = getDict()->lookup("ColorSpace");
        smaskInDataObj = getDict()->lookup("SMaskInData");
    }

    const int bufSize = streamBufferSize();

    bool indexed = false;
    if (cspace.isArrayOfLengthAtLeast(1)) {
        const Object cstype = cspace.arrayGet(0);
        if (cstype.isName("Indexed")) {
            indexed = true;
        }
    }

    const int smaskInData = smaskInDataObj.isInt() ? smaskInDataObj.getInt() : 0;
    const std::vector<unsigned char> buf = str->toUnsignedChars(bufSize);
    priv->init2(OPJ_CODEC_JP2, buf.data(), buf.size(), indexed);

    if (priv->image) {
        int numComps = priv->image->numcomps;
        int alpha = 0;
        if (priv->image->color_space == OPJ_CLRSPC_SRGB && numComps == 4) {
            numComps = 3;
            alpha = 1;
        } else if (priv->image->color_space == OPJ_CLRSPC_SYCC && numComps == 4) {
            numComps = 3;
            alpha = 1;
        } else if (numComps == 2) {
            numComps = 1;
            alpha = 1;
        } else if (numComps > 4) {
            numComps = 4;
            alpha = 1;
        } else {
            alpha = 0;
        }
        priv->npixels = priv->image->comps[0].w * priv->image->comps[0].h;
        priv->ncomps = priv->image->numcomps;
        if (alpha == 1 && smaskInData == 0 && !supportJPXtransparency()) {
            priv->ncomps--;
        }
        for (int component = 0; component < priv->ncomps; component++) {
            if (priv->image->comps[component].data == nullptr) {
                close();
                break;
            }
            const int componentPixels = priv->image->comps[component].w * priv->image->comps[component].h;
            if (componentPixels != priv->npixels) {
                error(errSyntaxWarning, -1, "Component {0:d} has different WxH than component 0", component);
                close();
                break;
            }
            auto *cdata = reinterpret_cast<unsigned char *>(priv->image->comps[component].data);
            int adjust = 0;
            int depth = priv->image->comps[component].prec;
            if (priv->image->comps[component].prec > 8) {
                adjust = priv->image->comps[component].prec - 8;
            }
            int sgndcorr = 0;
            if (priv->image->comps[component].sgnd) {
                sgndcorr = 1 << (priv->image->comps[0].prec - 1);
            }
            for (int i = 0; i < priv->npixels; i++) {
                int r = priv->image->comps[component].data[i];
                *(cdata++) = adjustComp(r, adjust, depth, sgndcorr, indexed);
            }
        }
    } else {
        priv->npixels = 0;
    }

    priv->counter = 0;
    priv->ccounter = 0;
    priv->inited = true;
}

void JPXStreamPrivate::init2(OPJ_CODEC_FORMAT format, const unsigned char *buf, int length, bool indexed)
{
    JPXData jpxData;

    jpxData.data = buf;
    jpxData.pos = 0;
    jpxData.size = length;

    opj_stream_t *stream;

    stream = opj_stream_default_create(OPJ_TRUE);

    opj_stream_set_user_data(stream, &jpxData, nullptr);

    opj_stream_set_read_function(stream, jpxRead_callback);
    opj_stream_set_skip_function(stream, jpxSkip_callback);
    opj_stream_set_seek_function(stream, jpxSeek_callback);
    /* Set the length to avoid an assert */
    opj_stream_set_user_data_length(stream, length);

    opj_codec_t *decoder;

    /* Use default decompression parameters */
    opj_dparameters_t parameters;
    opj_set_default_decoder_parameters(&parameters);
    if (indexed) {
        parameters.flags |= OPJ_DPARAMETERS_IGNORE_PCLR_CMAP_CDEF_FLAG;
    }

    /* Get the decoder handle of the format */
    decoder = opj_create_decompress(format);
    if (decoder == nullptr) {
        error(errSyntaxWarning, -1, "Unable to create decoder");
        goto error;
    }

    /* Catch events using our callbacks */
    opj_set_warning_handler(decoder, libopenjpeg_warning_callback, nullptr);
    opj_set_error_handler(decoder, libopenjpeg_error_callback, nullptr);

    /* Setup the decoder decoding parameters */
    if (!opj_setup_decoder(decoder, &parameters)) {
        error(errSyntaxWarning, -1, "Unable to set decoder parameters");
        goto error;
    }

    /* Decode the stream and fill the image structure */
    image = nullptr;
    if (!opj_read_header(stream, decoder, &image)) {
        error(errSyntaxWarning, -1, "Unable to read header");
        goto error;
    }

    /* Optional if you want decode the entire image */
    if (!opj_set_decode_area(decoder, image, parameters.DA_x0, parameters.DA_y0, parameters.DA_x1, parameters.DA_y1)) {
        error(errSyntaxWarning, -1, "X2");
        goto error;
    }

    /* Get the decoded image */
    if (!(opj_decode(decoder, stream, image) && opj_end_decompress(decoder, stream))) {
        error(errSyntaxWarning, -1, "Unable to decode image");
        goto error;
    }

    opj_destroy_codec(decoder);
    opj_stream_destroy(stream);

    if (image != nullptr) {
        return;
    }

error:
    if (image != nullptr) {
        opj_image_destroy(image);
        image = nullptr;
    }
    opj_stream_destroy(stream);
    opj_destroy_codec(decoder);
    if (format == OPJ_CODEC_JP2) {
        error(errSyntaxWarning, -1, "Did no succeed opening JPX Stream as JP2, trying as J2K.");
        init2(OPJ_CODEC_J2K, buf, length, indexed);
    } else if (format == OPJ_CODEC_J2K) {
        error(errSyntaxWarning, -1, "Did no succeed opening JPX Stream as J2K, trying as JPT.");
        init2(OPJ_CODEC_JPT, buf, length, indexed);
    } else {
        error(errSyntaxError, -1, "Did no succeed opening JPX Stream.");
    }
}
