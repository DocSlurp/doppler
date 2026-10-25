//========================================================================
//
// StructTreeRoot.cc
//
// This file is licensed under the GPLv2 or later
//
// Copyright 2013, 2014 Igalia S.L.
// Copyright 2014 Fabio D'Urso <fabiodurso@hotmail.it>
// Copyright 2017 Jan-Erik S <janerik234678@gmail.com>
// Copyright 2017-2019, 2023, 2025, 2026 Albert Astals Cid <aacid@kde.org>
// Copyright 2017, 2018 Adrian Johnson <ajohnson@redneon.com>
// Copyright 2018, Adam Reichold <adam.reichold@t-online.de>
// Copyright 2025, Kevin Backhouse <kevinbackhouse@github.com>
//
//========================================================================

#include "StructTreeRoot.h"
#include "StructElement.h"
#include "PDFDoc.h"
#include "Page.h"
#include "Object.h"
#include "Dict.h"
#include <cassert>

StructTreeRoot::StructTreeRoot(PDFDoc *docA, const Dict &structTreeRootDict) : doc(docA)
{
    assert(doc);
    parse(structTreeRootDict);
}

StructTreeRoot::~StructTreeRoot()
{
    for (StructElement *element : elements) {
        delete element;
    }
}

void StructTreeRoot::parse(const Dict &root)
{
    // The RoleMap/ClassMap dictionaries are needed by all the parsing
    // functions, which will resolve the custom names to canonical
    // standard names.
    roleMap = root.lookup("RoleMap");
    classMap = root.lookup("ClassMap");

    // ParentTree (optional). If present, it must be a number tree,
    // otherwise it is not possible to map stream objects to their
    // corresponding structure element. Here only the references are
    // loaded into the array, the pointers to the StructElements will
    // be filled-in later when parsing them.
    const Object parentTreeObj = root.lookup("ParentTree");
    if (parentTreeObj.isDict()) {
        RefRecursionChecker usedParents;
        parseNumberTreeNode(*parentTreeObj.getDict(), usedParents);
    }

    RefRecursionChecker seenElements;

    // Parse the children StructElements
    const bool marked = doc->getCatalog()->getMarkInfo() & Catalog::markInfoMarked;
    Object kids = root.lookup("K");
    if (kids.isArray()) {
        if (marked && kids.arrayGetLength() > 1) {
            error(errSyntaxWarning, -1, "K in StructTreeRoot has more than one children in a tagged PDF");
        }
        Array *kidsArray = kids.getArray();
        for (int i = 0; i < kidsArray->getLength(); i++) {
            const Object &ref = kidsArray->getNF(i);
            if (ref.isRef()) {
                seenElements.insert(ref.getRef());
            }
            Object obj = kidsArray->get(i);
            if (obj.isDict()) {
                auto *child = new StructElement(obj.getDict(), this, nullptr, seenElements);
                if (child->isOk()) {
                    if (marked && child->getType() != StructElement::Document && child->getType() != StructElement::Part && child->getType() != StructElement::Art && child->getType() != StructElement::Div) {
                        error(errSyntaxWarning, -1, "StructTreeRoot element of tagged PDF is wrong type ({0:s})", child->getTypeName());
                    }
                    appendChild(child);
                    if (ref.isRef()) {
                        parentTreeAdd(ref.getRef(), child);
                    }
                } else {
                    error(errSyntaxWarning, -1, "StructTreeRoot element could not be parsed");
                    delete child;
                }
            } else {
                error(errSyntaxWarning, -1, "K has a child of wrong type ({0:s})", obj.getTypeName());
            }
        }
    } else if (kids.isDict()) {
        auto *child = new StructElement(kids.getDict(), this, nullptr, seenElements);
        if (child->isOk()) {
            appendChild(child);
            const Object &ref = root.lookupNF("K");
            if (ref.isRef()) {
                parentTreeAdd(ref.getRef(), child);
            }
        } else {
            error(errSyntaxWarning, -1, "StructTreeRoot element could not be parsed");
            delete child;
        }
    } else if (!kids.isNull()) {
        error(errSyntaxWarning, -1, "K in StructTreeRoot is wrong type ({0:s})", kids.getTypeName());
    }

    // refToParentMap is only used during parsing. Ensure all memory used by it is freed.
    std::multimap<Ref, Parent *>().swap(refToParentMap);
}

void StructTreeRoot::parseNumberTreeNode(const Dict &node, RefRecursionChecker &usedParents)
{
    Object kids = node.lookup("Kids");
    if (kids.isArray()) {
        for (int i = 0; i < kids.arrayGetLength(); i++) {
            Ref ref;
            const Object obj = kids.getArray()->get(i, &ref);
            if (!usedParents.insert(ref)) {
                return;
            }
            const RefRecursionCheckerRemover remover(usedParents, ref);
            if (obj.isDict()) {
                parseNumberTreeNode(*obj.getDict(), usedParents);
            } else {
                error(errSyntaxError, -1, "Kids item at position {0:d} is wrong type ({1:s})", i, obj.getTypeName());
            }
        }
        return;
    }
    if (!kids.isNull()) {
        error(errSyntaxError, -1, "Kids object is wrong type ({0:s})", kids.getTypeName());
    }

    Object nums = node.lookup("Nums");
    if (nums.isArray()) {
        Array *numsArray = nums.getArray();
        if (numsArray->getLength() % 2 == 0) {
            // keys in even positions, references in odd ones
            for (int i = 0; i < numsArray->getLength(); i += 2) {
                Object key = numsArray->get(i);

                if (!key.isInt()) {
                    error(errSyntaxError, -1, "Nums item at position {0:d} is wrong type ({1:s})", i, key.getTypeName());
                    continue;
                }
                int keyVal = key.getInt();
                std::vector<Parent> &vec = parentTree[keyVal];
                if (!vec.empty()) {
                    error(errSyntaxError, -1, "Nums item at position {0:d} is a duplicate entry for key {1:d}", i, keyVal);
                    continue;
                }

                Object valueArrayObj = numsArray->get(i + 1);
                if (valueArrayObj.isArray()) {
                    Array *valueArray = valueArrayObj.getArray();
                    vec.resize(valueArray->getLength());
                    for (int j = 0; j < valueArray->getLength(); j++) {
                        const Object &itemvalue = valueArray->getNF(j);
                        if (itemvalue.isRef()) {
                            Ref ref = itemvalue.getRef();
                            vec[j].ref = ref;
                            refToParentMap.insert(std::pair<Ref, Parent *>(ref, &vec[j]));
                        } else if (!itemvalue.isNull()) {
                            error(errSyntaxError, -1, "Nums array item at position {0:d}/{1:d} is invalid type ({2:s})", i, j, itemvalue.getTypeName());
                        }
                    }
                } else {
                    const Object &valueRef = numsArray->getNF(i + 1);
                    if (valueRef.isRef()) {
                        Ref ref = valueRef.getRef();
                        vec.resize(1);
                        vec[0].ref = ref;
                        refToParentMap.insert(std::pair<Ref, Parent *>(ref, vec.data()));
                    } else {
                        error(errSyntaxError, -1, "Nums item at position {0:d} is wrong type ({1:s})", i + 1, valueRef.getTypeName());
                    }
                }
            }
        } else {
            error(errSyntaxError, -1, "Nums array length is not a even ({0:d})", numsArray->getLength());
        }
    } else {
        error(errSyntaxError, -1, "Nums object is wrong type ({0:s})", nums.getTypeName());
    }
}

void StructTreeRoot::parentTreeAdd(const Ref objectRef, StructElement *element)
{
    auto range = refToParentMap.equal_range(objectRef);
    for (auto it = range.first; it != range.second; ++it) {
        it->second->element = element;
    }
}

const MarkedContentOutputDev::CollectedSpans &StructTreeRoot::getPageSpans(int pageNum)
{
    const std::scoped_lock lock(cacheMutex);
    auto it = pageSpans.find(pageNum);
    if (it == pageSpans.end()) {
        // Interpret the page once and keep the text of all its MCIDs.
        MarkedContentOutputDev mcdev;
        doc->displayPages(&mcdev, pageNum, pageNum, 72.0, 72.0, 0, true, false, false);
        it = pageSpans.emplace(pageNum, mcdev.takeCollectedSpans()).first;
    }
    return it->second;
}

const TextSpanArray &StructTreeRoot::getMarkedContentSpans(int pageNum, Ref stm, int mcid)
{
    const std::scoped_lock lock(cacheMutex);
    static const TextSpanArray noSpans;

    if (pageNum < 1 || pageNum > doc->getNumPages()) {
        return noSpans;
    }
    const MarkedContentOutputDev::CollectedSpans &spans = getPageSpans(pageNum);
    auto it = spans.find(MarkedContentOutputDev::Key(stm, mcid));
    return it == spans.end() ? noSpans : it->second;
}

void StructTreeRoot::buildReverseIndex()
{
    reverseIndexBuilt = true;
    for (const auto &[key, parents] : parentTree) {
        for (size_t mcid = 0; mcid < parents.size(); mcid++) {
            if (parents[mcid].element) {
                mcidToKeys[std::make_pair(static_cast<const StructElement *>(parents[mcid].element), static_cast<int>(mcid))].push_back(key);
            }
        }
    }
    for (int i = 1; i <= doc->getNumPages(); i++) {
        const Page *page = doc->getPage(i);
        if (page && page->getStructParents() >= 0) {
            structParentsToPage.emplace(page->getStructParents(), i);
        }
    }
}

int StructTreeRoot::findMarkedContentPage(const StructElement *parent, Ref stm, int mcid)
{
    const std::scoped_lock lock(cacheMutex);
    const int numPages = doc->getNumPages();
    if (numPages == 1) {
        return 1;
    }

    // The ParentTree maps the StructParents key of a page (and the MCID) to
    // the structure element which holds the marked content.
    if (parent && stm == Ref::INVALID()) {
        if (!reverseIndexBuilt) {
            buildReverseIndex();
        }
        auto it = mcidToKeys.find(std::make_pair(parent, mcid));
        if (it != mcidToKeys.end()) {
            int found = 0;
            for (int key : it->second) {
                auto pageIt = structParentsToPage.find(key);
                if (pageIt != structParentsToPage.end()) {
                    if (found && found != pageIt->second) {
                        found = 0;
                        break;
                    }
                    found = pageIt->second;
                }
            }
            if (found) {
                return found;
            }
        }
    }

    // Last resort: accept the only page which has this marked content.
    int found = 0;
    for (int i = 1; i <= numPages; i++) {
        const MarkedContentOutputDev::CollectedSpans &spans = getPageSpans(i);
        if (spans.find(MarkedContentOutputDev::Key(stm, mcid)) != spans.end()) {
            if (found) {
                return 0;
            }
            found = i;
        }
    }
    return found;
}
