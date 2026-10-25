//========================================================================
//
// StructTreeRoot.h
//
// This file is licensed under the GPLv2 or later
//
// Copyright 2013, 2014 Igalia S.L.
// Copyright 2018, 2019, 2024-2026 Albert Astals Cid <aacid@kde.org>
// Copyright 2018 Adrian Johnson <ajohnson@redneon.com>
// Copyright 2018 Adam Reichold <adam.reichold@t-online.de>
//
//========================================================================

#ifndef STRUCTTREEROOT_H
#define STRUCTTREEROOT_H

#include "Object.h"
#include "StructElement.h"
#include <map>
#include <mutex>
#include <vector>

class Dict;
class PDFDoc;

class POPPLER_PRIVATE_EXPORT StructTreeRoot
{
public:
    StructTreeRoot(PDFDoc *docA, const Dict &rootDict);
    ~StructTreeRoot();

    StructTreeRoot &operator=(const StructTreeRoot &) = delete;
    StructTreeRoot(const StructTreeRoot &) = delete;

    PDFDoc *getDoc() { return doc; }
    Dict *getRoleMap() { return roleMap.isDict() ? roleMap.getDict() : nullptr; }
    Dict *getClassMap() { return classMap.isDict() ? classMap.getDict() : nullptr; }
    unsigned getNumChildren() const { return elements.size(); }
    const StructElement *getChild(int i) const { return elements.at(i); }
    StructElement *getChild(int i) { return elements.at(i); }

    void appendChild(StructElement *element)
    {
        if (element && element->isOk()) {
            elements.push_back(element);
        }
    }

    // Text of the marked content sequence with the given MCID, in the
    // content stream <stm> (Ref::INVALID() for page content) of page
    // <pageNum>. The content of each page is interpreted at most once, the
    // result is kept for the lifetime of the tree. Empty if there is none.
    const TextSpanArray &getMarkedContentSpans(int pageNum, Ref stm, int mcid);

    // Page number to which the marked content <mcid> of the structure
    // element <parent> belongs, deduced when /Pg is missing: from the
    // ParentTree if possible, otherwise from the only page that has such
    // marked content. Zero if it cannot be determined.
    int findMarkedContentPage(const StructElement *parent, Ref stm, int mcid);

    const StructElement *findParentElement(int key, unsigned mcid = 0) const
    {
        auto it = parentTree.find(key);
        if (it != parentTree.end()) {
            if (mcid < it->second.size()) {
                return it->second[mcid].element;
            }
        }
        return nullptr;
    }

private:
    using ElemPtrArray = std::vector<StructElement *>;

    // Structure for items in /ParentTree, it keeps a mapping of
    // object references and pointers to StructElement objects.
    struct Parent
    {
        Ref ref = Ref::INVALID();
        StructElement *element = nullptr;
    };

    PDFDoc *doc;
    Object roleMap;
    Object classMap;
    ElemPtrArray elements;
    std::map<int, std::vector<Parent>> parentTree;
    std::multimap<Ref, Parent *> refToParentMap;

    // Guards the lazily filled caches below: StructElement::getText() is
    // const and may be called from several threads on the same document.
    // Recursive because findMarkedContentPage() fills pageSpans too.
    std::recursive_mutex cacheMutex;
    // Text of marked content, per page (see getMarkedContentSpans())
    std::map<int, MarkedContentOutputDev::CollectedSpans> pageSpans;
    // Reverse ParentTree: (element, mcid) -> ParentTree keys; built on demand
    std::map<std::pair<const StructElement *, int>, std::vector<int>> mcidToKeys;
    std::map<int, int> structParentsToPage;
    bool reverseIndexBuilt = false;

    const MarkedContentOutputDev::CollectedSpans &getPageSpans(int pageNum);
    void buildReverseIndex();

    void parse(const Dict &rootDict);
    void parseNumberTreeNode(const Dict &node, RefRecursionChecker &usedParents);
    void parentTreeAdd(Ref objectRef, StructElement *element);

    friend class StructElement;
};

#endif
