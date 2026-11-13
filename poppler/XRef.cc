//========================================================================
//
// XRef.cc
//
// Copyright 1996-2003, 2025 Glyph & Cog, LLC
//
//========================================================================

//========================================================================
//
// Modified under the Poppler project - http://poppler.freedesktop.org
//
// All changes made under the Poppler project to this file are licensed
// under GPL version 2 or later
//
// Copyright (C) 2005 Dan Sheridan <dan.sheridan@postman.org.uk>
// Copyright (C) 2005 Brad Hards <bradh@frogmouth.net>
// Copyright (C) 2006, 2008, 2010, 2012-2014, 2016-2026 Albert Astals Cid <aacid@kde.org>
// Copyright (C) 2007-2008 Julien Rebetez <julienr@svn.gnome.org>
// Copyright (C) 2007 Carlos Garcia Campos <carlosgc@gnome.org>
// Copyright (C) 2009, 2010 Ilya Gorenbein <igorenbein@finjan.com>
// Copyright (C) 2010 Hib Eris <hib@hiberis.nl>
// Copyright (C) 2012, 2013, 2016 Thomas Freitag <Thomas.Freitag@kabelmail.de>
// Copyright (C) 2012, 2013 Fabio D'Urso <fabiodurso@hotmail.it>
// Copyright (C) 2013, 2014, 2017, 2019 Adrian Johnson <ajohnson@redneon.com>
// Copyright (C) 2013 Pino Toscano <pino@kde.org>
// Copyright (C) 2016 Jakub Alba <jakubalba@gmail.com>
// Copyright (C) 2018, 2019 Adam Reichold <adam.reichold@t-online.de>
// Copyright (C) 2018 Tobias Deiminger <haxtibal@posteo.de>
// Copyright (C) 2019 LE GARREC Vincent <legarrec.vincent@gmail.com>
// Copyright (C) 2020 Klarälvdalens Datakonsult AB, a KDAB Group company, <info@kdab.com>. Work sponsored by Technische Universität Dresden
// Copyright (C) 2010 William Bader <william@newspapersystems.com>
// Copyright (C) 2021 Mahmoud Khalil <mahmoudkhalil11@gmail.com>
// Copyright (C) 2021 Georgiy Sgibnev <georgiy@sgibnev.com>. Work sponsored by lab50.net.
// Copyright (C) 2023, 2025, 2026 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright (C) 2023 Ilaï Deutel <idtl@google.com>
// Copyright (C) 2023 Even Rouault <even.rouault@spatialys.com>
// Copyright (C) 2024 Nelson Benítez León <nbenitezl@gmail.com>
// Copyright (C) 2024 Vincent Lefevre <vincent@vinc17.net>
// Copyright (C) 2025 Arnav V <arnav0872@gmail.com>
// Copyright (C) 2026 Ojas Maheshwari <workonlyojas@gmail.com>
// Copyright (C) 2026 Adam Sampson <ats@offog.org>
// Copyright (C) 2026 Stefan Brüns <stefan.bruens@rwth-aachen.de>
// Copyright (C) 2026 Dirk Müller <dmueller@suse.com>
//
// To see a description of the changes please see the Changelog file that
// came with your tarball or type make ChangeLog if you are building from git
//
//========================================================================

#include <config.h>

#include <algorithm>
#include <array>
#include <cstdlib>
#include <cstddef>
#include <cstring>
#include <cctype>
#include <climits>
#include <unordered_set>
#include <limits>
#include "goo/gfile.h"
#include "goo/gmem.h"
#include "Object.h"
#include "Stream.h"
#include "Lexer.h"
#include "Parser.h"
#include "Dict.h"
#include "Error.h"
#include "ErrorCodes.h"
#include "XRef.h"

//------------------------------------------------------------------------
// Permission bits
// Note that the PDF spec uses 1 base (eg bit 3 is 1<<2)
//------------------------------------------------------------------------

constexpr int permPrint = 1 << 2; // bit 3
constexpr int permChange = 1 << 3; // bit 4
constexpr int permCopy = 1 << 4; // bit 5
constexpr int permNotes = 1 << 5; // bit 6
constexpr int permFillForm = 1 << 8; // bit 9
constexpr int permAccessibility = 1 << 9; // bit 10
constexpr int permAssemble = 1 << 10; // bit 11
constexpr int permHighResPrint = 1 << 11; // bit 12
constexpr int defPermFlags = 0xfffc;

//------------------------------------------------------------------------
// ObjectStream
//------------------------------------------------------------------------

class ObjectStream
{
public:
    // Create an object stream, using object number <objStrNum>,
    // generation 0.
    ObjectStream(XRef *xref, int objStrNumA, int recursion = 0);

    bool isOk() const { return ok; }

    ~ObjectStream();

    ObjectStream(const ObjectStream &) = delete;
    ObjectStream &operator=(const ObjectStream &) = delete;

    // Return the object number of this object stream.
    int getObjStrNum() const { return objStrNum; }

    // Get the <objIdx>th object from this stream, which should be
    // object number <objNum>, generation 0.
    Object getObject(int objIdx, int objNum);

private:
    int objStrNum; // object number of the object stream
    int nObjects; // number of objects in the stream
    Object *objs; // the objects (length = nObjects)
    int *objNums; // the object numbers (length = nObjects)
    bool ok;
};

ObjectStream::ObjectStream(XRef *xref, int objStrNumA, int recursion)
{
    Parser *parser;
    Goffset *offsets;
    Object obj1;
    Goffset first;
    int i;

    objStrNum = objStrNumA;
    nObjects = 0;
    objs = nullptr;
    objNums = nullptr;
    ok = false;

    const Object objStr = xref->fetch(objStrNum, 0, recursion);
    if (!objStr.isStream()) {
        return;
    }
    Stream *objStrStream = objStr.getStream();
    Dict *objStrStreamDict = objStrStream->getDict();

    obj1 = objStrStreamDict->lookup("N", recursion);
    if (!obj1.isInt()) {
        return;
    }
    nObjects = obj1.getInt();
    if (nObjects <= 0) {
        return;
    }

    obj1 = objStrStreamDict->lookup("First", recursion);
    if (!obj1.isInt() && !obj1.isInt64()) {
        return;
    }
    if (obj1.isInt()) {
        first = obj1.getInt();
    } else {
        first = obj1.getInt64();
    }
    if (first < 0) {
        return;
    }

    // this is an arbitrary limit to avoid integer overflow problems
    // in the 'new Object[nObjects]' call (Acrobat apparently limits
    // object streams to 100-200 objects)
    if (nObjects > 1000000) {
        error(errSyntaxError, -1, "Too many objects in an object stream");
        return;
    }
    if (!objStrStream->rewind()) {
        return;
    }
    objs = new Object[nObjects];
    objNums = static_cast<int *>(gmallocn(nObjects, sizeof(int)));
    offsets = static_cast<Goffset *>(gmallocn(nObjects, sizeof(Goffset)));

    // parse the header: object numbers and offsets
    auto embedStr = std::make_unique<EmbedStream>(objStrStream, Object::null(), true, first);
    parser = new Parser(xref, std::move(embedStr), false);
    for (i = 0; i < nObjects; ++i) {
        obj1 = parser->getObj();
        Object obj2 = parser->getObj();
        if (!obj1.isInt() || !(obj2.isInt() || obj2.isInt64())) {
            delete parser;
            gfree(offsets);
            return;
        }
        objNums[i] = obj1.getInt();
        if (obj2.isInt()) {
            offsets[i] = obj2.getInt();
        } else {
            offsets[i] = obj2.getInt64();
        }
        if (objNums[i] < 0 || offsets[i] < 0 || (i > 0 && offsets[i] < offsets[i - 1])) {
            delete parser;
            gfree(offsets);
            return;
        }
    }
    {
        auto *str = parser->getStream();
        while (str && str->getChar() != EOF) {
            ;
        }
    }
    delete parser;

    // skip to the first object - this shouldn't be necessary because
    // the First key is supposed to be equal to offsets[0], but just in
    // case...
    for (Goffset pos = first; pos < offsets[0]; ++pos) {
        objStr.getStream()->getChar();
    }

    // parse the objects
    for (i = 0; i < nObjects; ++i) {
        std::unique_ptr<Stream> strPtr;
        if (i == nObjects - 1) {
            strPtr = std::make_unique<EmbedStream>(objStr.getStream(), Object::null(), false, 0);
        } else {
            strPtr = std::make_unique<EmbedStream>(objStr.getStream(), Object::null(), true, offsets[i + 1] - offsets[i]);
        }
        parser = new Parser(xref, std::move(strPtr), false);
        objs[i] = parser->getObj();
        auto *str = parser->getStream();
        while (str && str->getChar() != EOF) {
            ;
        }
        delete parser;
    }

    gfree(offsets);
    ok = true;
}

ObjectStream::~ObjectStream()
{
    delete[] objs;
    gfree(objNums);
}

Object ObjectStream::getObject(int objIdx, int objNum)
{
    if (objIdx < 0 || objIdx >= nObjects || objNum != objNums[objIdx]) {
        return Object::null();
    }
    return objs[objIdx].copy();
}

//------------------------------------------------------------------------
// XRef
//------------------------------------------------------------------------

#define xrefLocker() const std::scoped_lock locker(mutex)

XRef::XRef()
{
    ok = true;
    errCode = errNone;
    entries = nullptr;
    capacity = 0;
    size = 0;
    modified = false;
    streamEnds = nullptr;
    streamEndsLen = 0;
    mainXRefEntriesOffset = 0;
    xRefStream = false;
    scannedSpecialFlags = false;
    encrypted = false;
    permFlags = defPermFlags;
    ownerPasswordOk = false;
    rootNum = -1;
    rootGen = -1;
    xrefReconstructed = false;
    encAlgorithm = cryptNone;
    keyLength = 0;
}

XRef::XRef(const Object *trailerDictA) : XRef {}
{
    if (trailerDictA->isDict()) {
        trailerDict = trailerDictA->copy();
    }
}

XRef::XRef(BaseStream *strA, Goffset pos, Goffset mainXRefEntriesOffsetA, bool *wasReconstructed, bool reconstruct, const std::function<void()> &xrefReconstructedCallback) : XRef {}
{
    Object obj;

    mainXRefEntriesOffset = mainXRefEntriesOffsetA;

    xrefReconstructedCb = xrefReconstructedCallback;

    // read the trailer
    str = strA;
    start = str->getStart();
    prevXRefOffset = mainXRefOffset = pos;

    if (reconstruct && !(ok = constructXRef(wasReconstructed))) {
        errCode = errDamaged;
        return;
    }
    // if there was a problem with the 'startxref' position, try to
    // reconstruct the xref table
    if (prevXRefOffset == 0) {
        if (!(ok = constructXRef(wasReconstructed))) {
            errCode = errDamaged;
            return;
        }

        // read the xref table
    } else {
        // The main section and the /Prev chain share one parse budget (readPrevChain): twice the
        // file plus 64 KiB, so nested /XRefStm or overlapping sections cannot re-read the file
        // once per level.
        const Goffset fileLength = str->getLength();
        chainBytesLeft = (fileLength > 0 && fileLength < (Goffset(1) << 40) ? 2 * fileLength : 0) + 65536;
        std::vector<Goffset> followedXRefStm;
        const bool more = readXRef(&prevXRefOffset, &followedXRefStm, nullptr);
        if (!ok) {
            chainBytesLeft = -1;
            chainBudgetHit = false;
        }

        // if there was a problem with the xref table,
        // try to reconstruct it
        if (!ok) {
            if (!(ok = constructXRef(wasReconstructed))) {
                errCode = errDamaged;
                return;
            }
        } else if (xRefStream || !mainXRefEntriesOffset) {
            // A linearized file's classic first-page table is the exception: getEntry reads its
            // entries straight from the main table and never follows the chain.
            if (!more) {
                prevXRefOffset = 0; // readXRef leaves the position alone when there is no /Prev
            }
            readPrevChain(wasReconstructed);
        } else {
            chainBytesLeft = -1; // the linearized first-page case: no chain read
            chainBudgetHit = false;
            if (!ok) {
                errCode = errDamaged;
                return;
            }
        }
    }

    // set size to (at least) the size specified in trailer dict
    obj = trailerDict.dictLookupNF("Size").copy();
    if (!obj.isInt()) {
        error(errSyntaxWarning, -1, "No valid XRef size in trailer");
    } else {
        if (obj.getInt() > size) {
            if (resize(obj.getInt()) != obj.getInt()) {
                if (!(ok = constructXRef(wasReconstructed))) {
                    errCode = errDamaged;
                    return;
                }
            }
        }
    }

    // get the root dictionary (catalog) object
    obj = trailerDict.dictLookupNF("Root").copy();
    if (obj.isRef()) {
        rootNum = obj.getRefNum();
        rootGen = obj.getRefGen();
    } else {
        if (!(ok = constructXRef(wasReconstructed))) {
            errCode = errDamaged;
            return;
        }
    }

    // now set the trailer dictionary's xref pointer so we can fetch
    // indirect objects from it
    trailerDict.getDict()->setXRef(this);
}

XRef::~XRef()
{
    for (int i = 0; i < size; i++) {
        if (entries[i].type == xrefEntryFree) {
            continue;
        }

        entries[i].obj.~Object();
    }
    gfree(entries);

    if (streamEnds) {
        gfree(streamEnds);
    }
}

std::unique_ptr<XRef> XRef::copy() const
{
    auto xref = std::make_unique<XRef>();
    xref->strOwner = str->copy();
    xref->str = xref->strOwner.get();
    xref->encrypted = encrypted;
    xref->permFlags = permFlags;
    xref->ownerPasswordOk = ownerPasswordOk;
    xref->rootGen = rootGen;
    xref->rootNum = rootNum;

    xref->start = start;
    xref->prevXRefOffset = prevXRefOffset;
    xref->mainXRefOffset = mainXRefOffset;
    xref->rebuildOnMissing = rebuildOnMissing;
    xref->xrefReconstructed = xrefReconstructed;
    xref->mainXRefEntriesOffset = mainXRefEntriesOffset;
    xref->xRefStream = xRefStream;
    xref->trailerDict = trailerDict.copy();
    xref->encAlgorithm = encAlgorithm;
    xref->encRevision = encRevision;
    xref->encVersion = encVersion;
    xref->permFlags = permFlags;
    xref->keyLength = keyLength;
    xref->permFlags = permFlags;
    for (int i = 0; i < 32; i++) {
        xref->fileKey[i] = fileKey[i];
    }

    if (xref->reserve(size) == 0) {
        error(errSyntaxError, -1, "unable to allocate {0:d} entries", size);
        return nullptr;
    }
    xref->size = size;
    for (int i = 0; i < size; ++i) {
        xref->entries[i].offset = entries[i].offset;
        xref->entries[i].type = entries[i].type;
        // set the object to null, it will be fetched from the stream when needed
        new (&xref->entries[i].obj) Object(Object::null());
        xref->entries[i].flags = entries[i].flags;
        xref->entries[i].gen = entries[i].gen;

        // If entry has been changed from the stream value we need to copy it
        // otherwise it's lost
        if (entries[i].getFlag(XRefEntry::Updated)) {
            xref->entries[i].obj = entries[i].obj.copy();
        }
    }
    xref->streamEndsLen = streamEndsLen;
    if (streamEndsLen != 0) {
        xref->streamEnds = static_cast<Goffset *>(gmalloc(streamEndsLen * sizeof(Goffset)));
        for (int i = 0; i < streamEndsLen; i++) {
            xref->streamEnds[i] = streamEnds[i];
        }
    }
    return xref;
}

int XRef::reserve(int newSize)
{
    if (newSize > capacity) {
        int newCapacity = 1024;
        if (capacity) {
            if (capacity <= INT_MAX / 2) {
                newCapacity = capacity * 2;
            } else {
                newCapacity = newSize;
            }
        }
        while (newSize > newCapacity) {
            if (newCapacity > INT_MAX / 2) {
                std::fputs("Too large XRef size\n", stderr);
                return 0;
            }
            newCapacity *= 2;
        }
        if (newCapacity >= INT_MAX / static_cast<int>(sizeof(XRefEntry))) {
            std::fputs("Too large XRef size\n", stderr);
            return 0;
        }

        void *p = grealloc(entries, newCapacity * sizeof(XRefEntry),
                           /* checkoverflow=*/true);
        if (p == nullptr) {
            return 0;
        }

        entries = static_cast<XRefEntry *>(p);
        capacity = newCapacity;
    }

    return capacity;
}

int XRef::resize(int newSize)
{
    if (newSize > size) {

        if (reserve(newSize) < newSize) {
            return size;
        }

        for (int i = size; i < newSize; ++i) {
            entries[i].offset = -1;
            entries[i].type = xrefEntryNone;
            new (&entries[i].obj) Object(Object::null());
            entries[i].flags = 0;
            entries[i].gen = 0;
        }
    } else {
        for (int i = newSize; i < size; i++) {
            entries[i].obj.~Object();
        }
    }

    size = newSize;

    return size;
}

/* Read one xref table section.  Also reads the associated trailer
 * dictionary, and returns the prev pointer (if any).
 * Arguments:
 *   pos                Points to a Goffset containing the offset of the XRef
 *                      section to be read. If a prev pointer is found, *pos is
 *                      updated with its value
 *   followedXRefStm    Used in case of nested readXRef calls to spot circular
 *                      references in XRefStm pointers
 *   xrefStreamObjsNum  If not NULL, every time a XRef stream is encountered,
 *                      its object number is appended
 * Return value:
 *   true if a prev pointer is found, otherwise false
 */
bool XRef::readXRef(Goffset *pos, std::vector<Goffset> *followedXRefStm, std::vector<int> *xrefStreamObjsNum)
{
    Parser *parser;
    Object obj;
    bool more;

    Goffset parsePos;
    if (unlikely(checkedAdd(start, *pos, &parsePos))) {
        ok = false;
        return false;
    }
    if (parsePos < 0) {
        ok = false;
        return false;
    }

    // start up a parser, parse one token. While readPrevChain runs, the parser sees only the
    // bytes left of its budget, and what it reads is taken off the budget, so sections that
    // overlap cannot make the chain re-read the file once per section.
    const bool budgeted = chainBytesLeft >= 0;
    if (budgeted && chainBytesLeft <= 0) {
        noteChainBudget("parse");
        ok = false;
        return false;
    }
    // an xref stream's predictor row is a few bytes (its /W widths); a larger one is refused
    // before it is allocated (StreamPredictor)
    const PredictorRowLimit rowLimit(budgeted ? 4096 : 0);
    const unsigned long rowRefusalsBefore = PredictorRowLimit::refusals();
    parser = new Parser(nullptr, str->makeSubStream(parsePos, budgeted, budgeted ? chainBytesLeft : 0, Object::null()), true);
    Goffset chargedFrom = parsePos;
    auto chargeBytes = [&] {
        if (budgeted) {
            chargeChainBytes(parser->getPos(), &chargedFrom);
        }
    };
    obj = parser->getObj(true);

    // parse an old-style xref table
    if (obj.isCmd("xref")) {
        more = readXRefTable(parser, pos, followedXRefStm, xrefStreamObjsNum, budgeted ? &chargedFrom : nullptr);

        // parse an xref stream
    } else if (obj.isInt()) {
        const int objNum = obj.getInt();
        if (obj = parser->getObj(true), !obj.isInt()) {
            goto err1;
        }
        if (obj = parser->getObj(true), !obj.isCmd("obj")) {
            goto err1;
        }
        if (obj = parser->getObj(), !obj.isStream()) {
            goto err1;
        }
        if (trailerDict.isNone()) {
            xRefStream = true;
        }
        if (xrefStreamObjsNum) {
            xrefStreamObjsNum->push_back(objNum);
        }
        more = readXRefStream(obj.getStream(), pos);

    } else {
        goto err1;
    }

    chargeBytes();
    delete parser;
    if (budgeted && PredictorRowLimit::refusals() != rowRefusalsBefore) {
        // a predictor row past the limit was dropped: the section was not read as written
        noteChainBudget("predictor");
        ok = false;
        return false;
    }
    return more;

err1:
    chargeBytes();
    delete parser;
    ok = false;
    return false;
}

void XRef::noteChainBudget(const char *what)
{
    if (!chainBudgetHit) {
        chainBudgetName = what;
    }
    chainBudgetHit = true;
}

void XRef::chargeChainBytes(Goffset after, Goffset *chargedFrom)
{
    // Takes what the parser read since *chargedFrom off the parse budget. A parser that reached
    // the end of its substream (the budget) spent it; one that stopped short did not.
    if (chainBytesLeft < 0 || !chargedFrom) {
        return;
    }
    // the lexer reports -1 once it ran off the end of its substream: all of it was read
    const Goffset used = after < 0 ? chainBytesLeft : after > *chargedFrom ? after - *chargedFrom : 0;
    if (after > *chargedFrom) {
        *chargedFrom = after;
    }
    if (used >= chainBytesLeft) {
        chainBytesLeft = 0;
        noteChainBudget("parse");
    } else {
        chainBytesLeft -= used;
    }
}

bool XRef::readXRefTable(Parser *parser, Goffset *pos, std::vector<Goffset> *followedXRefStm, std::vector<int> *xrefStreamObjsNum, Goffset *chargedFrom)
{
    XRefEntry entry;
    bool more;
    Object obj, obj2;
    Goffset pos2;
    int first, n;

    while (true) {
        obj = parser->getObj(true);
        if (obj.isCmd("trailer")) {
            break;
        }
        if (!obj.isInt()) {
            goto err0;
        }
        first = obj.getInt();
        obj = parser->getObj(true);
        if (!obj.isInt()) {
            goto err0;
        }
        n = obj.getInt();
        if (first < 0 || n < 0 || first > INT_MAX - n) {
            goto err0;
        }
        if (!chargeChainEntries(first, n)) {
            goto err0;
        }
        if (first + n > size) {
            if (resize(first + n) != first + n) {
                error(errSyntaxError, -1, "Invalid 'obj' parameters'");
                goto err0;
            }
        }
        for (int i = first; i < first + n; ++i) {
            obj = parser->getObj(true);
            if (obj.isInt()) {
                entry.offset = obj.getInt();
            } else if (obj.isInt64()) {
                entry.offset = obj.getInt64();
            } else {
                goto err0;
            }
            obj = parser->getObj(true);
            if (!obj.isInt()) {
                goto err0;
            }
            entry.gen = obj.getInt();
            entry.flags = 0;
            obj = parser->getObj(true);
            if (obj.isCmd("n")) {
                entry.type = xrefEntryUncompressed;
            } else if (obj.isCmd("f")) {
                entry.type = xrefEntryFree;
            } else {
                goto err0;
            }
            if (entries[i].offset == -1) {
                entries[i].offset = entry.offset;
                entries[i].gen = entry.gen;
                entries[i].type = entry.type;
                entries[i].flags = entry.flags;
                entries[i].obj.setToNull();
                if (chainFilled) {
                    chainFilled->push_back(i);
                    if (i == 1) {
                        chainFilled->push_back(0); // the IBM workaround below may move it to 0
                    }
                }

                // PDF files of patents from the IBM Intellectual Property
                // Network have a bug: the xref table claims to start at 1
                // instead of 0.
                if (i == 1 && first == 1 && entries[1].offset == 0 && entries[1].gen == 65535 && entries[1].type == xrefEntryFree) {
                    i = first = 0;
                    entries[0].offset = 0;
                    entries[0].gen = 65535;
                    entries[0].type = xrefEntryFree;
                    entries[0].flags = entries[1].flags;
                    entries[0].obj = std::move(entries[1].obj);

                    entries[1].offset = -1;
                    entries[1].obj.setToNull();
                }
            }
            if (i > last) {
                last = i;
            }
        }
    }

    // read the trailer dictionary
    obj = parser->getObj();
    if (!obj.isDict()) {
        goto err0;
    }

    // get the 'Prev' pointer
    obj2 = obj.getDict()->lookupNF("Prev").copy();
    if (obj2.isInt() || obj2.isInt64()) {
        if (obj2.isInt()) {
            pos2 = obj2.getInt();
        } else {
            pos2 = obj2.getInt64();
        }
        if (pos2 != *pos) {
            *pos = pos2;
            more = true;
        } else {
            error(errSyntaxWarning, -1, "Infinite loop in xref table");
            more = false;
        }
    } else if (obj2.isRef()) {
        // certain buggy PDF generators generate "/Prev NNN 0 R" instead
        // of "/Prev NNN"
        pos2 = static_cast<unsigned int>(obj2.getRefNum());
        if (pos2 != *pos) {
            *pos = pos2;
            more = true;
        } else {
            error(errSyntaxWarning, -1, "Infinite loop in xref table");
            more = false;
        }
    } else {
        more = false;
    }

    // save the first trailer dictionary
    if (trailerDict.isNone()) {
        trailerDict = obj.copy();
    }

    // check for an 'XRefStm' key
    obj2 = obj.getDict()->lookup("XRefStm");
    if (obj2.isInt() || obj2.isInt64()) {
        if (obj2.isInt()) {
            pos2 = obj2.getInt();
        } else {
            pos2 = obj2.getInt64();
        }
        for (size_t i = 0; ok && i < followedXRefStm->size(); ++i) {
            if (followedXRefStm->at(i) == pos2) {
                ok = false;
            }
        }
        // Arbitrary limit because otherwise we exhaust the stack
        // calling readXRef + readXRefTable
        if (followedXRefStm->size() > 4096) {
            error(errSyntaxError, -1, "File has more than 4096 XRefStm, aborting");
            ok = false;
        }
        if (ok) {
            followedXRefStm->push_back(pos2);
            // charge this section's bytes so far first, so a nested section's parser sees only
            // what is left of the budget
            chargeChainBytes(parser->getPos(), chargedFrom);
            if (chainBudgetHit) {
                ok = false;
            } else {
                readXRef(&pos2, followedXRefStm, xrefStreamObjsNum);
            }
        }
        if (!ok) {
            goto err0;
        }
    }

    return more;

err0:
    ok = false;
    return false;
}

bool XRef::readXRefStream(Stream *xrefStr, Goffset *pos)
{
    int w[3];
    bool more;
    Object obj;

    ok = false;

    Dict *dict = xrefStr->getDict();
    obj = dict->lookupNF("Size").copy();
    if (!obj.isInt()) {
        return false;
    }
    int newSize = obj.getInt();
    if (newSize < 0) {
        return false;
    }
    if (!chargeChainEntries(0, 0, newSize)) {
        return false;
    }
    if (newSize > size) {
        if (resize(newSize) != newSize) {
            error(errSyntaxError, -1, "Invalid 'size' parameter");
            return false;
        }
    }

    obj = dict->lookupNF("W").copy();
    if (!obj.isArrayOfLengthAtLeast(3)) {
        return false;
    }
    for (int i = 0; i < 3; ++i) {
        Object obj2 = obj.arrayGet(i);
        if (!obj2.isInt()) {
            return false;
        }
        w[i] = obj2.getInt();
        if (w[i] < 0) {
            return false;
        }
    }

    constexpr int intNBytes = sizeof(int);
    constexpr int longLongNBytes = sizeof(long long);
    if (w[0] > intNBytes || w[1] > longLongNBytes || w[2] > longLongNBytes) {
        return false;
    }

    if (!xrefStr->rewind()) {
        return false;
    }

    const Object &idx = dict->lookupNF("Index");
    if (idx.isArray()) {
        for (int i = 0; i + 1 < idx.arrayGetLength(); i += 2) {
            obj = idx.arrayGet(i);
            if (!obj.isInt()) {
                return false;
            }
            int first = obj.getInt();
            obj = idx.arrayGet(i + 1);
            if (!obj.isInt()) {
                return false;
            }
            int n = obj.getInt();
            if (first < 0 || n < 0 || !readXRefStreamSection(xrefStr, w, first, n)) {
                return false;
            }
        }
    } else {
        if (!readXRefStreamSection(xrefStr, w, 0, newSize)) {
            return false;
        }
    }

    obj = dict->lookupNF("Prev").copy();
    if (obj.isInt() && obj.getInt() >= 0) {
        *pos = obj.getInt();
        more = true;
    } else if (obj.isInt64() && obj.getInt64() >= 0) {
        *pos = obj.getInt64();
        more = true;
    } else {
        more = false;
    }
    if (trailerDict.isNone()) {
        trailerDict = xrefStr->getDictObject()->copy();
    }

    ok = true;
    return more;
}

bool XRef::readXRefStreamSection(Stream *xrefStr, const int *w, int first, int n)
{
    unsigned long long offset, gen;
    const unsigned long long offsetMax = GoffsetMax();

    if (first > INT_MAX - n) {
        return false;
    }
    if (first + n < 0) {
        return false;
    }
    if (!chargeChainEntries(first, n)) {
        return false;
    }
    if (first + n > size) {
        if (resize(first + n) != size) {
            error(errSyntaxError, -1, "Invalid 'size' inside xref table");
            return false;
        }
        if (first + n > size) {
            error(errSyntaxError, -1, "Invalid 'first' or 'n' inside xref table");
            return false;
        }
    }

    constexpr int intNBytes = sizeof(int);
    constexpr int longLongNBytes = sizeof(long long);
    const int nChars = w[0] + w[1] + w[2];
    std::array<unsigned char, intNBytes + 2 * longLongNBytes> xrefEntryBuffer;

    for (int i = first; i < first + n; ++i) {
        if (xrefStr->doGetChars(nChars, xrefEntryBuffer.data()) < nChars) {
            return false;
        }

        int type = 1;
        int j = 0;

        if (w[0] != 0) {
            for (type = 0; j < w[0]; ++j) {
                const unsigned char c = xrefEntryBuffer[j];
                type = (type << 8) + c;
            }
        }
        for (offset = 0; j < (w[1] + w[0]); ++j) {
            const unsigned char c = xrefEntryBuffer[j];
            offset = (offset << 8) + c;
        }
        if (offset > offsetMax) {
            error(errSyntaxError, -1, "Offset inside xref table too large for fseek");
            return false;
        }
        for (gen = 0; j < (w[2] + w[1] + w[0]); ++j) {
            const unsigned char c = xrefEntryBuffer[j];
            gen = (gen << 8) + c;
        }
        if (gen > INT_MAX) {
            if (i == 0 && gen == std::numeric_limits<uint32_t>::max()) {
                // workaround broken generators
                gen = 65535;
            } else {
                error(errSyntaxError, -1, "Gen inside xref table too large (bigger than INT_MAX)");
                return false;
            }
        }
        if (entries[i].offset == -1) {
            if (chainFilled && type >= 0 && type <= 2) {
                chainFilled->push_back(i);
            }
            switch (type) {
            case 0:
                entries[i].offset = offset;
                entries[i].gen = static_cast<int>(gen);
                entries[i].type = xrefEntryFree;
                break;
            case 1:
                entries[i].offset = offset;
                entries[i].gen = static_cast<int>(gen);
                entries[i].type = xrefEntryUncompressed;
                break;
            case 2:
                entries[i].offset = offset;
                entries[i].gen = static_cast<int>(gen);
                entries[i].type = xrefEntryCompressed;
                break;
            default:
                return false;
            }
        }
    }

    return true;
}

// Attempt to construct an xref table for a damaged file.
bool XRef::constructXRef(bool *wasReconstructed, bool needCatalogDict)
{
    ++generation;
    int *streamObjNums = nullptr;
    int streamObjNumsLen = 0;
    int streamObjNumsSize = 0;
    int lastObjNum = -1;
    rootNum = -1;
    int streamEndsSize = 0;
    streamEndsLen = 0;

    resize(0); // free entries properly
    gfree(entries);
    capacity = 0;
    size = 0;
    last = -1;
    entries = nullptr;

    if (wasReconstructed) {
        *wasReconstructed = true;
    }
    ++reconstructions;
    // DocSlurp (issue 45): a rebuilt table is not extended later by a lazy read of the old chain,
    // and object streams parsed before it (under the old table's offsets) are not reused
    prevXRefOffset = 0;
    objStrs.clear();
    {
        const Goffset fileLength = str->getLength();
        trailerBytesLeft = (fileLength > 0 && fileLength < (Goffset(1) << 40) ? 2 * fileLength : 0) + 65536;
    }

    if (xrefReconstructedCb) {
        xrefReconstructedCb();
    }

    if (!str->rewind()) {
        return false;
    }
    char buf[4096 + 1];

    intptr_t bufPos = start;
    char *p = buf;
    char *end = buf;
    bool startOfLine = true;
    bool space = true;
    bool eof = false;
    while (true) {
        if (end - p < 256 && !eof) {
            memmove(buf, p, end - p);
            bufPos += p - buf;
            p = buf + (end - p);
            int n = static_cast<int>(buf + 4096 - p);
            int m = str->doGetChars(n, reinterpret_cast<unsigned char *>(p));
            end = p + m;
            *end = '\0';
            p = buf;
            eof = m < n;
        }
        if (p == end && eof) {
            break;
        }
        if (startOfLine && !strncmp(p, "trailer", 7)) {
            constructTrailerDict(static_cast<intptr_t>(bufPos + (p + 7 - buf)), needCatalogDict);
            p += 7;
            startOfLine = false;
            space = false;
        } else if (startOfLine && !strncmp(p, "endstream", 9)) {
            if (streamEndsLen == streamEndsSize) {
                streamEndsSize += 64;
                streamEnds = static_cast<Goffset *>(greallocn(streamEnds, streamEndsSize, sizeof(Goffset)));
            }
            streamEnds[streamEndsLen++] = (static_cast<Goffset>(bufPos) + (p - buf));
            p += 9;
            startOfLine = false;
            space = false;
        } else if (space && *p >= '0' && *p <= '9') {
            p = constructObjectEntry(p, (static_cast<Goffset>(bufPos) + (p - buf)), &lastObjNum);
            startOfLine = false;
            space = false;
        } else if (p[0] == '>' && p[1] == '>') {
            p += 2;
            startOfLine = false;
            space = false;
            // skip any PDF whitespace except for '\0'
            while (*p == '\t' || *p == '\n' || *p == '\x0c' || *p == '\r' || *p == ' ') {
                if (*p == '\n' || *p == '\r') {
                    startOfLine = true;
                }
                space = true;
                ++p;
            }
            if (!strncmp(p, "stream", 6)) {
                if (lastObjNum >= 0) {
                    if (streamObjNumsLen == streamObjNumsSize) {
                        streamObjNumsSize += 64;
                        streamObjNums = static_cast<int *>(greallocn(streamObjNums, streamObjNumsSize, sizeof(int)));
                    }
                    streamObjNums[streamObjNumsLen++] = lastObjNum;
                }
                p += 6;
                startOfLine = false;
                space = false;
            }
        } else {
            if (*p == '\n' || *p == '\r') {
                startOfLine = true;
                space = true;
            } else if (Lexer::isSpace(*p & 0xff)) {
                space = true;
            } else {
                startOfLine = false;
                space = false;
            }
            ++p;
        }
    }

    // read each stream object, check for xref or object stream
    for (int i = 0; i < streamObjNumsLen; ++i) {
        Object obj = fetch(streamObjNums[i], entries[streamObjNums[i]].gen);
        if (obj.isStream()) {
            Dict *dict = obj.getStream()->getDict();
            Object type = dict->lookup("Type");
            if (type.isName("XRef")) {
                saveTrailerDict(dict, true, needCatalogDict);
            } else if (type.isName("ObjStm")) {
                constructObjectStreamEntries(&obj, streamObjNums[i]);
            }
        }
    }

    gfree(streamObjNums);

    if (rootNum < 0) {
        error(errSyntaxError, -1, "Couldn't find trailer dictionary");
        return false;
    }
    return true;
}

// Attempt to construct a trailer dict at [pos] in the stream.
void XRef::constructTrailerDict(Goffset pos, bool needCatalogDict)
{
    // DocSlurp (issue 45): the trailers a rebuild finds are parsed within one budget of twice the
    // file plus 64 KiB (set by constructXRef), so many trailers whose dictionaries run over the
    // rest of the file cannot make the rebuild re-read it once per trailer. Past the budget the
    // trailers already read stand.
    if (trailerBytesLeft <= 0) {
        return;
    }
    Parser parser { nullptr, str->makeSubStream(pos, true, trailerBytesLeft, Object {}), false };
    Object newTrailerDict = parser.getObj();
    const Goffset after = parser.getPos();
    // -1: the lexer ran off the end of its substream, which the budget cut
    const Goffset used = after < 0 ? trailerBytesLeft : after > pos ? after - pos : 0;
    trailerBytesLeft = used >= trailerBytesLeft ? 0 : trailerBytesLeft - used;
    if (newTrailerDict.isDict()) {
        saveTrailerDict(newTrailerDict.getDict(), false, needCatalogDict);
    }
}

// If [dict] "looks like" a trailer dict (i.e., has a Root entry),
// save it as the trailer dict.
void XRef::saveTrailerDict(Dict *dict, bool isXRefStream, bool needCatalogDict)
{
    const Object &obj = dict->lookupNF("Root");
    if (obj.isRef() && (rootNum == -1 || !needCatalogDict)) {
        int newRootNum = obj.getRefNum();
        // the xref stream scanning code runs after all objects are found,
        // so we can check for a valid root object number at that point
        if (!isXRefStream || newRootNum <= last) {
            rootNum = newRootNum;
            rootGen = obj.getRefGen();
            trailerDict = Object { dict->copy(this) };
        }
    }
}

// Look for an object header ("nnn ggg obj") at [p].  The first
// character at *[p] is a digit.  [pos] is the position of *[p].
char *XRef::constructObjectEntry(char *p, Goffset pos, int *objNum)
{
    // we look for non-end-of-line space characters here, to deal with
    // situations like:
    //    nnn          <-- garbage digits on a line
    //    nnn nnn obj  <-- actual object
    // and we also ignore '\0' (because it's used to terminate the
    // buffer in this damage-scanning code)
    // though some documents seems to also have nnn\nnn\nobj\n so
    // in those cases, we should not actually consume the newline from
    // the data if this is not a object entry like that.
    ptrdiff_t ateNewLines = 0;
    int num = 0;
    do {
        num = (num * 10) + (*p - '0');
        ++p;
    } while (std::isdigit(static_cast<unsigned char>(*p)) && num < 100000000);
    if (!std::isspace(static_cast<unsigned char>(*p))) {
        return p;
    }
    do {
        if (*p == '\n' || ateNewLines > 0) {
            ateNewLines++;
        }
        ++p;
    } while (std::isspace(static_cast<unsigned char>(*p)));
    if (*p < '0' || *p > '9') {
        if (ateNewLines > 0) {
            p -= ateNewLines;
        }
        return p;
    }
    int gen = 0;
    do {
        gen = (gen * 10) + (*p - '0');
        if (ateNewLines > 0) {
            ateNewLines++;
        }
        ++p;
    } while (std::isdigit(static_cast<unsigned char>(*p)) && gen < 100000000);
    if (!std::isspace(static_cast<unsigned char>(*p))) {
        return p;
    }
    do {
        if (*p == '\n' || ateNewLines > 0) {
            ateNewLines++;
        }
        ++p;
    } while (std::isspace(static_cast<unsigned char>(*p)));
    if (strncmp(p, "obj", 3) != 0) {
        if (ateNewLines > 0) {
            p -= ateNewLines;
        }
        return p;
    }

    if (constructXRefEntry(num, gen, pos - start, xrefEntryUncompressed)) {
        *objNum = num;
    }

    return p;
}

// Read the header from an object stream, and add xref entries for all
// of its objects.
void XRef::constructObjectStreamEntries(Object *objStr, int objStrObjNum)
{
    Object objN = objStr->getStream()->getDict()->lookup("N");

    // get the object count
    if (!objN.isInt()) {
        return;
    }
    int nObjects = objN.getInt();
    if (nObjects <= 0 || nObjects > 1000000) {
        return;
    }

    // parse the header: object numbers and offsets
    Parser parser { nullptr, objStr, false };
    for (int i = 0; i < nObjects; ++i) {
        auto obj1 = parser.getObj(true);
        auto obj2 = parser.getObj(true);
        if (obj1.isInt() && obj2.isInt()) {
            int num = obj1.getInt();
            if (num >= 0 && num < 1000000) {
                constructXRefEntry(num, i, objStrObjNum, xrefEntryCompressed);
            }
        }
    }
}

bool XRef::constructXRefEntry(int num, int gen, Goffset pos, XRefEntryType type)
{
    if (num >= size) {
        const int newSize = (num + 1 + 255) & ~255;
        if (newSize < 0) {
            return false;
        }
        if (resize(newSize) != newSize) {
            return false;
        }
    }

    if (entries[num].type == xrefEntryFree || gen >= entries[num].gen) {
        entries[num].offset = pos;
        entries[num].gen = gen;
        entries[num].type = type;
        if (num > last) {
            last = num;
        }
    }

    return true;
}

void XRef::setEncryption(int permFlagsA, bool ownerPasswordOkA, const unsigned char *fileKeyA, int keyLengthA, int encVersionA, int encRevisionA, CryptAlgorithm encAlgorithmA)
{
    int i;

    encrypted = true;
    permFlags = permFlagsA;
    ownerPasswordOk = ownerPasswordOkA;
    if (keyLengthA <= 32) {
        keyLength = keyLengthA;
    } else {
        keyLength = 32;
    }
    for (i = 0; i < keyLength; ++i) {
        fileKey[i] = fileKeyA[i];
    }
    encVersion = encVersionA;
    encRevision = encRevisionA;
    encAlgorithm = encAlgorithmA;
}

void XRef::getEncryptionParameters(unsigned char **fileKeyA, CryptAlgorithm *encAlgorithmA, int *keyLengthA)
{
    if (encrypted) {
        *fileKeyA = fileKey;
        *encAlgorithmA = encAlgorithm;
        *keyLengthA = keyLength;
    } else {
        // null encryption parameters
        *fileKeyA = nullptr;
        *encAlgorithmA = cryptRC4;
        *keyLengthA = 0;
    }
}

bool XRef::isRefEncrypted(Ref r)
{
    xrefLocker();

    const XRefEntry *e = getEntry(r.num);
    if (!e->obj.isNull()) { // check for updated object
        return false;
    }

    switch (e->type) {
    case xrefEntryUncompressed: {
        return encrypted && !e->getFlag(XRefEntry::Unencrypted);
    }

    case xrefEntryCompressed: {
        const Goffset objStrNum = e->offset;
        if (unlikely(objStrNum < 0 || objStrNum >= size)) {
            error(errSyntaxError, -1, "XRef::isRefEncrypted - Compressed object offset out of xref bounds");
            return false;
        }
        const Object objStr = fetch(static_cast<int>(e->offset), 0);
        return objStr.getStream()->isEncrypted();
    }

    default: {
    }
    }

    return false;
}

bool XRef::okToPrint(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permPrint);
}

// we can print at high res if we are only doing security handler revision
// 2 (and we are allowed to print at all), or with security handler rev
// 3 and we are allowed to print, and bit 12 is set.
bool XRef::okToPrintHighRes(bool ignoreOwnerPW) const
{
    if (encrypted) {
        if (2 == encRevision) {
            return (okToPrint(ignoreOwnerPW));
        }
        if (encRevision >= 3) {
            return (okToPrint(ignoreOwnerPW) && (permFlags & permHighResPrint));
        }
        // something weird - unknown security handler version
        return false;
    }
    return true;
}

bool XRef::okToChange(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permChange);
}

bool XRef::okToCopy(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permCopy);
}

bool XRef::okToAddNotes(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permNotes);
}

bool XRef::okToFillForm(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permFillForm);
}

bool XRef::okToAccessibility(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permAccessibility);
}

bool XRef::okToAssemble(bool ignoreOwnerPW) const
{
    return (!ignoreOwnerPW && ownerPasswordOk) || (permFlags & permAssemble);
}

Object XRef::getCatalog()
{
    Object catalog = fetch(rootNum, rootGen);
    if (catalog.isDict()) {
        return catalog;
    }
    if (repairSuppressed) {
        ++suppressedRepairs; // RepairGuard
        error(errSyntaxError, -1, "Catalog is broken; not rebuilt for a read-only lookup");
        return catalog;
    }
    bool wasReconstructed = false;
    if (constructXRef(&wasReconstructed, true)) {
        catalog = fetch(rootNum, rootGen);
    }
    return catalog;
}

Object XRef::fetch(const Ref ref, int recursion)
{
    return fetch(ref.num, ref.gen, recursion);
}

Object XRef::fetchStreamParam(const Ref ref, int recursion)
{
    const std::scoped_lock locker(mutex);
    if (streamParamsGeneration != generation) {
        streamParams.clear();
        streamParamsGeneration = generation;
    }
    const auto it = streamParams.find(ref);
    if (it != streamParams.end()) {
        return it->second.copy();
    }
    const unsigned int suppressedBefore = suppressedRepairs;
    Object obj = fetch(ref, recursion);
    if ((obj.isName() || obj.isArray() || obj.isDict()) && streamParamsGeneration == generation && suppressedRepairs == suppressedBefore && streamParams.size() < 4096) {
        streamParams.emplace(ref, obj.copy());
    }
    return obj;
}

Object XRef::fetch(int num, int gen, int recursion, Goffset *endPos)
{
    XRefEntry *e;
    Object obj1, obj2, obj3;

    xrefLocker();

    const Ref ref = { .num = num, .gen = gen };

    if (!refsBeingFetched.insert(ref)) {
        return Object::null();
    }

    // Will remove ref from refsBeingFetched once it's destroyed, i.e. the function returns
    auto remover = std::make_unique<RefRecursionCheckerRemover>(refsBeingFetched, ref);

    // check for bogus ref - this can happen in corrupted PDF files
    if (num < 0 || num >= size) {
        goto err;
    }

    e = getEntry(num);
    if (!e->obj.isNull()) { // check for updated object
        return e->obj.copy();
    }

    switch (e->type) {

    case xrefEntryUncompressed: {
        if (e->gen != gen || e->offset < 0) {
            goto err;
        }
        Goffset subStreamOffset;
        if (checkedAdd(start, e->offset, &subStreamOffset)) {
            goto err;
        }
        Parser parser { this, str->makeSubStream(subStreamOffset, false, 0, Object::null()), true };
        obj1 = parser.getObj(recursion);
        obj2 = parser.getObj(recursion);
        obj3 = parser.getObj(recursion);
        if (!obj1.isInt() || obj1.getInt() != num || !obj2.isInt() || obj2.getInt() != gen || !obj3.isCmd("obj")) {
            // some buggy pdf have obj1234 for ints that represent 1234
            // try to recover here
            if (obj1.isInt() && obj1.getInt() == num && obj2.isInt() && obj2.getInt() == gen && obj3.isCmd()) {
                const char *cmd = obj3.getCmd();
                if (strlen(cmd) > 3 && cmd[0] == 'o' && cmd[1] == 'b' && cmd[2] == 'j') {
                    char *end_ptr;
                    long longNumber = strtol(cmd + 3, &end_ptr, 0);
                    if (longNumber <= INT_MAX && longNumber >= INT_MIN && *end_ptr == '\0') {
                        int number = longNumber;
                        error(errSyntaxWarning, -1, "Cmd was not obj but {0:s}, assuming the creator meant obj {1:d}", cmd, number);
                        if (endPos) {
                            *endPos = parser.getPos();
                        }
                        return Object(number);
                    }
                }
            }
            goto err;
        }
        Object obj = parser.getObj(false, (encrypted && !e->getFlag(XRefEntry::Unencrypted)) ? fileKey : nullptr, encAlgorithm, keyLength, num, gen, recursion);
        if (endPos) {
            *endPos = parser.getPos();
        }
        return obj;
    }

    case xrefEntryCompressed: {
#define FOLLOW_SPECIFICATION_STRICTLY 0 // Adobe apparently ignores the generation number on compressed objects
#if FOLLOW_SPECIFICATION_STRICTLY
        if (gen != 0) {
            goto err;
        }
#endif
        if (e->offset >= static_cast<unsigned int>(size) || (entries[e->offset].type != xrefEntryUncompressed && entries[e->offset].type != xrefEntryNone)) {
            error(errSyntaxError, -1, "Invalid object stream");
            goto err;
        }

        ObjectStream *objStr = nullptr;
        if (const auto it = objStrs.find(e->offset); it != objStrs.end()) {
            objStr = it->second.get();
        } else if (repairSuppressed) {
            // RepairGuard: an object stream whose parse a refused repair shaped (a /Length that
            // would have needed a rebuild, say) is used once and not kept
            const unsigned int before = suppressedRepairs;
            std::unique_ptr<ObjectStream> once(new ObjectStream(this, static_cast<int>(e->offset), recursion + 1));
            if (!once->isOk()) {
                goto err;
            }
            if (endPos) {
                *endPos = -1;
            }
            Object result = once->getObject(e->gen, num);
            if (suppressedRepairs == before) {
                // read as an unguarded fetch would have read it; constructXRef drops it with the
                // table it was parsed under
                objStrs.emplace(e->offset, std::unique_ptr<ObjectStream>(once.release()));
            }
            return result;
        } else {
            objStr = new ObjectStream(this, static_cast<int>(e->offset), recursion + 1);
            if (!objStr->isOk()) {
                delete objStr;
                objStr = nullptr;
                goto err;
            } else {
                // XRef could be reconstructed in constructor of ObjectStream:
                e = getEntry(num);
                const auto [_, inserted] = objStrs.emplace(e->offset, std::unique_ptr<ObjectStream>(objStr));
                if (!inserted) {
                    // this happens when indeed the xref was reconstructed, and the reconstuction ended up adding an ObjectStream
                    // for that offset, so fetch the good objStr because the one we had was just deleted by emplace failing
                    objStr = objStrs.find(e->offset)->second.get();
                }
            }
        }
        if (endPos) {
            *endPos = -1;
        }
        return objStr->getObject(e->gen, num);
    }

    default:
        goto err;
    }

err:
    if (!xRefStream && !xrefReconstructed && repairSuppressed) {
        ++suppressedRepairs;
        error(errSyntaxError, -1, "Object {0:d} is broken or missing; not rebuilt for a read-only lookup", num);
    } else if (!xRefStream && !xrefReconstructed) {
        // Check if there has been any updated object, if there has been we can't reconstruct because that would mean losing the changes
        bool xrefHasChanges = false;
        for (int i = 0; i < size; i++) {
            if (entries[i].getFlag(XRefEntry::Updated)) {
                xrefHasChanges = true;
                break;
            }
        }
        if (xrefHasChanges) {
            error(errInternal, -1, "xref num {0:d} not found but needed, document has changes, reconstruct aborted", num);
            // pretend we constructed the xref, otherwise we will do this check again and again
            xrefReconstructed = true;
            return Object::null();
        }

        error(errInternal, -1, "xref num {0:d} not found but needed, try to reconstruct", num);
        rootNum = -1;
        constructXRef(&xrefReconstructed);
        remover.reset(); // Manually delete the remover since we're calling ourselves so recursion for this one is valid
        return fetch(num, gen, ++recursion, endPos);
    }
    if (endPos) {
        *endPos = -1;
    }
    return Object::null();
}

void XRef::lock()
{
    mutex.lock();
}

void XRef::unlock()
{
    mutex.unlock();
}

Object XRef::getDocInfo()
{
    return trailerDict.dictLookup("Info");
}

// Added for the pdftex project.
Object XRef::getDocInfoNF()
{
    return trailerDict.dictLookupNF("Info").copy();
}

Object XRef::createDocInfoIfNeeded(Ref *ref)
{
    Object obj = trailerDict.getDict()->lookup("Info", ref);
    getDocInfo();

    if (obj.isDict() && *ref != Ref::INVALID()) {
        // Info is valid if it's a dict and to pointed by an indirect reference
        return obj;
    }

    removeDocInfo();

    obj = Object(std::make_unique<Dict>(this));
    *ref = addIndirectObject(obj);
    trailerDict.dictSet("Info", Object(*ref));

    return obj;
}

void XRef::removeDocInfo()
{
    Object infoObjRef = getDocInfoNF();
    if (infoObjRef.isNull()) {
        return;
    }

    trailerDict.dictRemove("Info");

    if (likely(infoObjRef.isRef())) {
        removeIndirectObject(infoObjRef.getRef());
    }
}

bool XRef::getStreamEnd(Goffset streamStart, Goffset *streamEnd)
{
    int a, b, m;

    if (streamEndsLen == 0 || streamStart > streamEnds[streamEndsLen - 1]) {
        return false;
    }

    a = -1;
    b = streamEndsLen - 1;
    // invariant: streamEnds[a] < streamStart <= streamEnds[b]
    while (b - a > 1) {
        m = (a + b) / 2;
        if (streamStart <= streamEnds[m]) {
            b = m;
        } else {
            a = m;
        }
    }
    *streamEnd = streamEnds[b];
    return true;
}

int XRef::getNumEntry(Goffset offset)
{
    if (size > 0) {
        int res = 0;
        Goffset resOffset = getEntry(0)->offset;
        XRefEntry *e;
        for (int i = 1; i < size; ++i) {
            e = getEntry(i, false);
            if (e->type != xrefEntryFree && e->offset < offset && e->offset >= resOffset) {
                res = i;
                resOffset = e->offset;
            }
        }
        return res;
    }
    return -1;
}

void XRef::add(Ref ref, Goffset offs, bool used)
{
    ++generation;
    add(ref.num, ref.gen, offs, used);
}

bool XRef::add(int num, int gen, Goffset offs, bool used)
{
    xrefLocker();
    if (num >= size) {
        if (num >= capacity) {
            entries = static_cast<XRefEntry *>(greallocn_checkoverflow(entries, num + 1, sizeof(XRefEntry)));
            if (unlikely(entries == nullptr)) {
                size = 0;
                capacity = 0;
                return false;
            }

            capacity = num + 1;
        }
        for (int i = size; i < num + 1; ++i) {
            entries[i].offset = -1;
            entries[i].type = xrefEntryFree;
            new (&entries[i].obj) Object(Object::null());
            entries[i].flags = 0;
            entries[i].gen = 0;
        }
        size = num + 1;
    }
    XRefEntry *e = getEntry(num);
    e->gen = gen;
    e->obj.setToNull();
    e->flags = 0;
    if (used) {
        e->type = xrefEntryUncompressed;
        e->offset = offs;
    } else {
        e->type = xrefEntryFree;
        e->offset = 0;
    }
    return true;
}

void XRef::setModifiedObject(const Object *o, Ref r)
{
    ++generation;
    xrefLocker();
    if (r.num < 0 || r.num >= size) {
        error(errInternal, -1, "XRef::setModifiedObject on unknown ref: {0:d}, {1:d}", r.num, r.gen);
        return;
    }
    XRefEntry *e = getEntry(r.num);
    if (unlikely(e->type == xrefEntryFree)) {
        error(errInternal, -1, "XRef::setModifiedObject on ref: {0:d}, {1:d} that is marked as free. This will cause a memory leak", r.num, r.gen);
    }
    e->obj = o->copy();
    e->setFlag(XRefEntry::Updated, true);
    setModified();
}

Ref XRef::addIndirectObject(const Object &o)
{
    int entryIndexToUse = -1;
    for (int i = 1; entryIndexToUse == -1 && i < size; ++i) {
        XRefEntry *e = getEntry(i, false /* complainIfMissing */);
        if (e->type == xrefEntryFree && e->gen < 65535) {
            entryIndexToUse = i;
        }
    }

    XRefEntry *e;
    if (entryIndexToUse == -1) {
        entryIndexToUse = size;
        add(entryIndexToUse, 0, 0, false);
        e = getEntry(entryIndexToUse);
    } else {
        // reuse a free entry
        e = getEntry(entryIndexToUse);
        // we don't touch gen number, because it should have been
        // incremented when the object was deleted
    }
    e->type = xrefEntryUncompressed;
    e->obj = o.copy();
    e->setFlag(XRefEntry::Updated, true);
    setModified();

    Ref r;
    r.num = entryIndexToUse;
    r.gen = e->gen;
    return r;
}

void XRef::removeIndirectObject(Ref r)
{
    ++generation;
    xrefLocker();
    if (r.num < 0 || r.num >= size) {
        error(errInternal, -1, "XRef::removeIndirectObject on unknown ref: {0:d}, {1:d}", r.num, r.gen);
        return;
    }
    XRefEntry *e = getEntry(r.num);
    if (e->type == xrefEntryFree) {
        return;
    }
    e->obj = Object();
    e->type = xrefEntryFree;
    if (likely(e->gen < 65535)) {
        e->gen++;
    }
    e->setFlag(XRefEntry::Updated, true);
    setModified();
}

Ref XRef::addStreamObject(std::unique_ptr<Dict> dict, std::vector<char> buffer, StreamCompression compression)
{
    dict->add("Length", Object(static_cast<int>(buffer.size())));
    auto stream = std::make_unique<AutoFreeMemStream>(std::move(buffer), Object(std::move(dict)));
    stream->setFilterRemovalForbidden(true);
    switch (compression) {
    case StreamCompression::None:;
        break;
    case StreamCompression::Compress:
        stream->getDict()->add("Filter", Object::name("FlateDecode"));
        break;
    }
    return addIndirectObject(Object(std::move(stream)));
}

void XRef::writeXRef(XRef::XRefWriter *writer, bool writeAllEntries)
{
    // create free entries linked-list
    if (getEntry(0)->gen != 65535) {
        error(errInternal, -1, "XRef::writeXRef, entry 0 of the XRef is invalid (gen != 65535)");
    }
    int lastFreeEntry = 0;
    for (int i = 0; i < size; i++) {
        if (getEntry(i)->type == xrefEntryFree) {
            getEntry(lastFreeEntry)->offset = i;
            lastFreeEntry = i;
        }
    }
    getEntry(lastFreeEntry)->offset = 0;

    if (writeAllEntries) {
        writer->startSection(0, size);
        for (int i = 0; i < size; i++) {
            XRefEntry *e = getEntry(i);
            if (e->gen > 65535) {
                e->gen = 65535; // cap generation number to 65535 (required by PDFReference)
            }
            writer->writeEntry(e->offset, e->gen, e->type);
        }
    } else {
        int i = 0;
        while (i < size) {
            int j;
            for (j = i; j < size; j++) { // look for consecutive entries
                if ((getEntry(j)->type == xrefEntryFree) && (getEntry(j)->gen == 0)) {
                    break;
                }
            }
            if (j - i != 0) {
                writer->startSection(i, j - i);
                for (int k = i; k < j; k++) {
                    XRefEntry *e = getEntry(k);
                    if (e->gen > 65535) {
                        e->gen = 65535; // cap generation number to 65535 (required by PDFReference)
                    }
                    writer->writeEntry(e->offset, e->gen, e->type);
                }
                i = j;
            } else {
                ++i;
            }
        }
    }
}

XRef::XRefTableWriter::XRefTableWriter(OutStream *outStrA)
{
    outStr = outStrA;
}

void XRef::XRefTableWriter::startSection(int first, int count)
{
    outStr->printf("%i %i\r\n", first, count);
}

void XRef::XRefTableWriter::writeEntry(Goffset offset, int gen, XRefEntryType type)
{
    outStr->printf("%010lli %05i %c\r\n", static_cast<long long>(offset), gen, (type == xrefEntryFree) ? 'f' : 'n');
}

void XRef::writeTableToFile(OutStream *outStr, bool writeAllEntries)
{
    XRefTableWriter writer(outStr);
    outStr->printf("xref\r\n");
    writeXRef(&writer, writeAllEntries);
}

XRef::XRefStreamWriter::XRefStreamWriter(Array *indexA, GooString *stmBufA, int offsetSizeA)
{
    index = indexA;
    stmBuf = stmBufA;
    offsetSize = offsetSizeA;
}

void XRef::XRefStreamWriter::startSection(int first, int count)
{
    index->add(Object(first));
    index->add(Object(count));
}

void XRef::XRefStreamWriter::writeEntry(Goffset offset, int gen, XRefEntryType type)
{
    const int entryTotalSize = 1 + offsetSize + 2; /* type + offset + gen */
    char data[16];
    data[0] = (type == xrefEntryFree) ? 0 : 1;
    for (int i = offsetSize; i > 0; i--) {
        data[i] = offset & 0xff;
        offset >>= 8;
    }
    data[offsetSize + 1] = (gen >> 8) & 0xff;
    data[offsetSize + 2] = gen & 0xff;
    stmBuf->append(data, entryTotalSize);
}

XRef::XRefPreScanWriter::XRefPreScanWriter()
{
    hasOffsetsBeyond4GB = false;
}

void XRef::XRefPreScanWriter::startSection(int /*first*/, int /*count*/) { }

void XRef::XRefPreScanWriter::writeEntry(Goffset offset, int /*gen*/, XRefEntryType /*type*/)
{
    if (offset >= 0x100000000LL) {
        hasOffsetsBeyond4GB = true;
    }
}

void XRef::writeStreamToBuffer(GooString *stmBuf, Dict *xrefDict, XRef *xref)
{
    auto index = std::make_unique<Array>(xref);
    stmBuf->clear();

    // First pass: determine whether all offsets fit in 4 bytes or not
    XRefPreScanWriter prescan;
    writeXRef(&prescan, false);
    const int offsetSize = prescan.hasOffsetsBeyond4GB ? sizeof(Goffset) : 4;

    // Second pass: actually write the xref stream
    XRefStreamWriter writer(index.get(), stmBuf, offsetSize);
    writeXRef(&writer, false);

    xrefDict->set("Type", Object::name("XRef"));
    xrefDict->set("Index", Object(std::move(index)));
    auto wArray = std::make_unique<Array>(xref);
    wArray->add(Object(1));
    wArray->add(Object(offsetSize));
    wArray->add(Object(2));
    xrefDict->set("W", Object(std::move(wArray)));
}

bool XRef::parseEntry(Goffset offset, XRefEntry *entry)
{
    bool r;

    if (unlikely(entry == nullptr)) {
        return false;
    }

    Parser parser(nullptr, str->makeSubStream(offset, false, 20, Object::null()), true);

    Object obj1, obj2, obj3;
    if (((obj1 = parser.getObj(), obj1.isInt()) || obj1.isInt64()) && (obj2 = parser.getObj(), obj2.isInt()) && (obj3 = parser.getObj(), obj3.isCmd("n") || obj3.isCmd("f"))) {
        if (obj1.isInt64()) {
            entry->offset = obj1.getInt64();
        } else {
            entry->offset = obj1.getInt();
        }
        entry->gen = obj2.getInt();
        entry->type = obj3.isCmd("n") ? xrefEntryUncompressed : xrefEntryFree;
        entry->obj.setToNull();
        entry->flags = 0;
        r = true;
    } else {
        r = false;
    }

    return r;
}

int XRef::countSections(int maxSections, long long maxTokens)
{
    // Walks the /Prev chain from the main section with a parser of its own: entries are skipped
    // token by token, trailers and xref stream dictionaries are read, nothing is stored. A
    // rebuilt table has no chain to trust. Every section is read through a substream cut to the
    // bytes left of a budget of twice the file plus 64 KiB, so a chain whose sections overlap
    // (an entry token that is a string spanning the deeper sections, say) cannot make the walk
    // re-read the file once per section.
    if (reconstructions > 0 || !str) {
        return 0;
    }
    const Goffset fileLength = str->getLength();
    Goffset bytesLeft = (fileLength > 0 && fileLength < (Goffset(1) << 40) ? 2 * fileLength : 0) + 65536;
    std::vector<Goffset> seen;
    Goffset pos = mainXRefOffset;
    long long tokens = 0;
    while (pos > 0 && static_cast<int>(seen.size()) < maxSections) {
        if (std::find(seen.begin(), seen.end(), pos) != seen.end()) {
            return static_cast<int>(seen.size()); // a loop: count what was seen
        }
        seen.push_back(pos);
        Goffset parsePos;
        if (checkedAdd(start, pos, &parsePos) || parsePos < 0 || bytesLeft <= 0) {
            return 0;
        }
        Parser parser(nullptr, str->makeSubStream(parsePos, true, bytesLeft, Object::null()), true);
        Object obj = parser.getObj(true);
        Object dictObj;
        if (obj.isCmd("xref")) {
            for (;;) {
                if (++tokens > maxTokens) {
                    return 0;
                }
                obj = parser.getObj(true);
                if (obj.isCmd("trailer")) {
                    dictObj = parser.getObj();
                    break;
                }
                if (!obj.isInt()) {
                    return 0;
                }
                Object countObj = parser.getObj(true);
                if (!countObj.isInt() || countObj.getInt() < 0) {
                    return 0;
                }
                const long long entryTokens = 3LL * countObj.getInt();
                if (tokens + entryTokens > maxTokens) {
                    return 0;
                }
                for (long long i = 0; i < entryTokens; ++i) {
                    if (parser.getObj(true).isEOF()) {
                        return 0;
                    }
                }
                tokens += entryTokens;
            }
        } else if (obj.isInt()) {
            Object gen = parser.getObj(true);
            Object kw = parser.getObj(true);
            if (!gen.isInt() || !kw.isCmd("obj")) {
                return 0;
            }
            // a cross-reference stream: its dictionary (simpleOnly would stop at "<<"). The
            // parser has no xref, so an indirect /Length stays a reference and the stream's data
            // is skipped, not read.
            dictObj = parser.getObj(false);
            if (dictObj.isStream()) {
                dictObj = dictObj.getStream()->getDictObject()->copy();
            }
        } else {
            return 0;
        }
        if (!dictObj.isDict()) {
            return 0;
        }
        const Goffset after = parser.getPos();
        bytesLeft -= after > parsePos ? after - parsePos : bytesLeft;
        Object prev = dictObj.dictLookupNF("Prev").copy();
        if (prev.isInt() && prev.getInt() > 0) {
            pos = prev.getInt();
        } else if (prev.isInt64() && prev.getInt64() > 0) {
            pos = prev.getInt64();
        } else {
            pos = 0;
        }
    }
    return pos > 0 ? 0 : static_cast<int>(seen.size());
}

bool XRef::chargeChainEntries(int first, int n, int newSize)
{
    // While readPrevChain runs: the entries older sections list come out of one budget, and the
    // slots they add to the table (a larger /Size or first + n) out of another, both checked
    // before anything is allocated, so a hostile older section cannot grow the table far past
    // what the main section made it.
    if (chainListedLeft < 0) {
        return true;
    }
    if (n > chainListedLeft) {
        noteChainBudget("entry");
        return false;
    }
    chainListedLeft -= n;
    const long long end = std::max(static_cast<long long>(first) + n, static_cast<long long>(newSize));
    const long long growth = end > size ? end - size : 0;
    if (growth > chainGrowthLeft) {
        noteChainBudget("table-growth");
        return false;
    }
    chainGrowthLeft -= growth;
    return true;
}

void XRef::readPrevChain(bool * /*wasReconstructed*/)
{
    // DocSlurp: read every section older than the main one now, instead of when a missing entry
    // is first asked for, so the table no longer depends on fetch order. Bounds: at most 999
    // older sections (1,000 with the main one, as countSections), 30,000,000 entries listed
    // across them, table growth of at most max(65,536, the main section's /Size) slots, and twice
    // the file's bytes plus 64 KiB of parsing shared with the main section; an xref stream's
    // predictor row of at most 4,096 bytes.
    //
    // Where the walk stops early (a loop, a broken section, a bound), what it read stands, the
    // section it stopped in is dropped (as the lazy read never used a section it could not read;
    // behind an xref stream a broken section's entries are kept, as there), and the first fetch
    // of an entry no section read lists rebuilds the table, counted as a rebuild after open. A
    // file whose objects the sections read all list draws as it did upstream.
    constexpr int maxSections = 999;
    constexpr long long maxListed = 30000000LL; // countSections' 30M-token bound
    if (chainBytesLeft < 0) {
        const Goffset fileLength = str->getLength();
        chainBytesLeft = (fileLength > 0 && fileLength < (Goffset(1) << 40) ? 2 * fileLength : 0) + 65536;
    }
    long long mainSize = size;
    const Object &trailerSize = trailerDict.dictLookupNF("Size");
    if (trailerSize.isInt() && trailerSize.getInt() > mainSize) {
        mainSize = trailerSize.getInt();
    }
    chainListedLeft = maxListed;
    chainGrowthLeft = std::max<long long>(65536, mainSize);
    std::unordered_set<Goffset> seen;
    seen.insert(mainXRefOffset);
    int sections = 0;
    bool capped = false; // a bound ended the chain before its last section was read
    std::vector<int> filled;
    chainFilled = &filled;
    auto dropSection = [&] {
        for (const int i : filled) {
            if (i >= 0 && i < size) {
                entries[i].offset = -1;
                entries[i].gen = 0;
                entries[i].type = xrefEntryNone;
                entries[i].flags = 0;
                entries[i].obj.setToNull();
            }
        }
    };
    while (prevXRefOffset) {
        if (!seen.insert(prevXRefOffset).second) {
            error(errSyntaxError, -1, "Circular XRef");
            break;
        }
        if (++sections > maxSections) {
            error(errSyntaxError, -1, "More than {0:d} older xref sections: the rest are not read", maxSections);
            capped = true;
            break;
        }
        std::vector<Goffset> followedXRefStm;
        filled.clear();
        if (!readXRef(&prevXRefOffset, &followedXRefStm, nullptr)) {
            prevXRefOffset = 0;
        }
        if (chainBudgetHit) {
            error(errSyntaxError, -1, "XRef sections past the {0:s} budget: the rest are not read", chainBudgetName ? chainBudgetName : "read");
            ok = true;
            dropSection();
            capped = true;
            break;
        }
        if (!ok) {
            ok = true;
            error(errSyntaxError, -1, "Broken older xref section: the chain ends there");
            if (!xRefStream) {
                dropSection();
            }
            break;
        }
    }
    chainFilled = nullptr;
    chainBytesLeft = -1;
    chainListedLeft = -1;
    chainGrowthLeft = -1;
    chainBudgetHit = false;
    chainBudgetName = nullptr;
    prevXRefOffset = 0;
    // The lazy read re-read the main section on the first missing entry and, with a classic table,
    // rebuilt when no section listed it: kept for every classic table. Behind an xref stream it
    // never rebuilt (a loop or broken section just ended the chain, as here), but a bound is
    // ours: an entry may be in a section not read, so its first fetch rebuilds there too.
    rebuildOnMissing = !xRefStream || capped;
}

/* Traverse all XRef tables and, if untilEntryNum != -1, stop as soon as
 * untilEntryNum is found, or try to reconstruct the xref table if it's not
 * present in any xref.
 * If xrefStreamObjsNum is not NULL, it is filled with the list of the object
 * numbers of the XRef streams that have been traversed */
void XRef::readXRefUntil(int untilEntryNum, std::vector<int> *xrefStreamObjsNum)
{
    std::vector<Goffset> followedPrev;
    while (prevXRefOffset && (untilEntryNum == -1 || (untilEntryNum < size && entries[untilEntryNum].type == xrefEntryNone))) {
        bool followed = false;
        for (long long j : followedPrev) {
            if (j == prevXRefOffset) {
                followed = true;
                break;
            }
        }
        if (followed) {
            error(errSyntaxError, -1, "Circular XRef");
            if (!xRefStream && !(ok = constructXRef(nullptr))) {
                errCode = errDamaged;
            }
            break;
        }

        followedPrev.push_back(prevXRefOffset);

        std::vector<Goffset> followedXRefStm;
        if (!readXRef(&prevXRefOffset, &followedXRefStm, xrefStreamObjsNum)) {
            prevXRefOffset = 0;
        }

        // if there was a problem with the xref table, or we haven't found the entry
        // we were looking for, try to reconstruct the xref
        if (!ok || (!prevXRefOffset && untilEntryNum != -1 && entries[untilEntryNum].type == xrefEntryNone)) {
            if (!xRefStream && !(ok = constructXRef(nullptr))) {
                errCode = errDamaged;
                break;
            }
            break;
        }
    }
}

namespace {

struct DummyXRefEntry : XRefEntry
{
    DummyXRefEntry()
    {
        offset = -1;
        gen = 0;
        type = xrefEntryNone;
        flags = 0;
        obj = Object::null();
    }
};

DummyXRefEntry dummyXRefEntry;

}

XRefEntry *XRef::getEntry(int i, bool complainIfMissing)
{
    if (unlikely(i < 0)) {
        error(errInternal, -1, "Request for invalid XRef entry [{0:d}]", i);
        return &dummyXRefEntry;
    }

    if (i >= size || entries[i].type == xrefEntryNone) {

        if ((!xRefStream) && mainXRefEntriesOffset) {
            if (unlikely(i >= size)) {
                error(errInternal, -1, "Request for out-of-bounds XRef entry [{0:d}]", i);
                return &dummyXRefEntry;
            }

            if (!parseEntry(mainXRefEntriesOffset + 20 * i, &entries[i])) {
                error(errSyntaxError, -1, "Failed to parse XRef entry [{0:d}].", i);
                return &dummyXRefEntry;
            }
        } else {
            if (repairSuppressed) {
                // neither read the chain, nor rebuild, nor mark the entry free (RepairGuard)
                if (prevXRefOffset || (rebuildOnMissing && i < size && entries[i].type == xrefEntryNone)) {
                    ++suppressedRepairs;
                    error(errSyntaxError, -1, "XRef entry {0:d} is in no section; not rebuilt for a read-only lookup", i);
                }
                return &dummyXRefEntry;
            }
            if (prevXRefOffset) {
                // Read XRef tables until the entry we're looking for is found (only when the
                // chain was not read at open)
                readXRefUntil(i);
            } else if (rebuildOnMissing && i < size && entries[i].type == xrefEntryNone) {
                // No section lists it: rebuild, as the lazy read of the chain did, but now
                // whatever was fetched first, and counted as a rebuild after open.
                rebuildOnMissing = false;
                bool hasChanges = false;
                for (int k = 0; k < size && !hasChanges; ++k) {
                    hasChanges = entries[k].getFlag(XRefEntry::Updated);
                }
                if (hasChanges) {
                    // as fetch: a rebuild would lose the edits
                    error(errInternal, -1, "XRef entry {0:d} is in no section, document has changes, reconstruct aborted", i);
                } else {
                    error(errSyntaxError, -1, "XRef entry {0:d} is in no section, reconstructing", i);
                    if (!(ok = constructXRef(nullptr))) {
                        errCode = errDamaged;
                    }
                }
            }

            // We might have reconstructed the xref
            // Check again i is in bounds
            if (unlikely(i >= size)) {
                return &dummyXRefEntry;
            }

            if (entries[i].type == xrefEntryNone) {
                if (complainIfMissing) {
                    error(errSyntaxError, -1, "Invalid XRef entry {0:d}", i);
                }
                entries[i].type = xrefEntryFree;
            }
        }
    }

    return &entries[i];
}

// Recursively sets the Unencrypted flag in all referenced xref entries
void XRef::markUnencrypted(Object *obj)
{
    Object obj1;

    switch (obj->getType()) {
    case objArray: {
        Array *array = obj->getArray();
        for (int i = 0; i < array->getLength(); i++) {
            obj1 = array->getNF(i).copy();
            markUnencrypted(&obj1);
        }
        break;
    }
    case objStream:
    case objDict: {
        Dict *dict;
        if (obj->getType() == objStream) {
            Stream *stream = obj->getStream();
            dict = stream->getDict();
        } else {
            dict = obj->getDict();
        }
        for (int i = 0; i < dict->getLength(); i++) {
            obj1 = dict->getValNF(i).copy();
            markUnencrypted(&obj1);
        }
        break;
    }
    case objRef: {
        const Ref ref = obj->getRef();
        XRefEntry *e = getEntry(ref.num);
        if (e->getFlag(XRefEntry::Unencrypted)) {
            return; // We've already been here: prevent infinite recursion
        }
        e->setFlag(XRefEntry::Unencrypted, true);
        obj1 = fetch(ref);
        markUnencrypted(&obj1);
        break;
    }
    default:
        break;
    }
}

void XRef::scanSpecialFlags()
{
    if (scannedSpecialFlags) {
        return;
    }
    scannedSpecialFlags = true;

    // "Rewind" the XRef linked list, so that readXRefUntil re-reads all XRef
    // tables/streams, even those that had already been parsed
    prevXRefOffset = mainXRefOffset;

    std::vector<int> xrefStreamObjNums;
    if (!streamEndsLen) { // don't do it for already reconstructed xref
        readXRefUntil(-1 /* read all xref sections */, &xrefStreamObjNums);
    }
    // upstream: once the chain is drained, a missing entry is free (writers must not rebuild)
    rebuildOnMissing = false;

    // Mark object streams as DontRewrite, because we write each object
    // individually in full rewrite mode.
    for (int i = 0; i < size; ++i) {
        if (entries[i].type == xrefEntryCompressed) {
            const Goffset objStmNum = entries[i].offset;
            if (unlikely(objStmNum < 0 || objStmNum >= size)) {
                error(errSyntaxError, -1, "Compressed object offset out of xref bounds");
            } else {
                getEntry(static_cast<int>(objStmNum))->setFlag(XRefEntry::DontRewrite, true);
            }
        }
    }

    // Mark XRef streams objects as Unencrypted and DontRewrite
    for (const int objNum : xrefStreamObjNums) {
        getEntry(objNum)->setFlag(XRefEntry::Unencrypted, true);
        getEntry(objNum)->setFlag(XRefEntry::DontRewrite, true);
    }

    // Mark objects referred from the Encrypt dict as Unencrypted
    markUnencrypted();
}

void XRef::markUnencrypted()
{
    // Mark objects referred from the Encrypt dict as Unencrypted
    const Object &obj = trailerDict.dictLookupNF("Encrypt");
    if (obj.isRef()) {
        XRefEntry *e = getEntry(obj.getRefNum());
        e->setFlag(XRefEntry::Unencrypted, true);
    }
}

XRef::XRefWriter::~XRefWriter() = default;
