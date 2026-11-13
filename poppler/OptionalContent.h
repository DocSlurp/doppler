//========================================================================
//
// OptionalContent.h
//
// Copyright 2007 Brad Hards <bradh@kde.org>
// Copyright 2008 Carlos Garcia Campos <carlosgc@gnome.org>
// Copyright 2013, 2018, 2019, 2021, 2025 Albert Astals Cid <aacid@kde.org>
// Copyright 2018 Adam Reichold <adam.reichold@t-online.de>
// Copyright 2019 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright 2025 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright 2026 Adam Sampson <ats@offog.org>
//
// Released under the GPL (version 2, or later, at your option)
//
//========================================================================

#ifndef OPTIONALCONTENT_H
#define OPTIONALCONTENT_H

#include "Object.h"
#include "poppler_private_export.h"
#include <unordered_map>
#include <memory>
#include <mutex>

class GooString;
class XRef;

class OptionalContentGroup;

// DocSlurp: the optional-content operands a series of visibility evaluations may still visit
// (Gfx keeps one per page). The first answer for an OCMD on the page is charged the operands
// its evaluation visits (at least 1), whether it is evaluated or taken from the document-wide
// cache, so what a page shows does not depend on what ran before it in the process; later
// answers for it on the same page are free. Once the budget is spent, OCMDs not yet answered on
// the page count as visible without looking, and the first gives a warning (page facts:
// limit-hit).
struct OCVisibilityBudget
{
    static constexpr long perPage = 200000;
    long operandsLeft = perPage;
    bool limitHit = false;
    struct Answer
    {
        bool visible;
        bool limited;
    };
    // OCMDs answered (and charged) on this page, at most one entry per operand charged, under
    // group state <generation> (OptionalContentGroup::stateGeneration)
    std::unordered_map<Ref, Answer> answered;
    unsigned long generation = 0;
};

//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT OCGs
{
public:
    OCGs(const Object &ocgObject, XRef *xref);

    OCGs(const OCGs &) = delete;
    OCGs &operator=(const OCGs &) = delete;

    // Is OCGS valid?
    bool isOk() const { return ok; }

    bool hasOCGs() const;
    const std::unordered_map<Ref, std::unique_ptr<OptionalContentGroup>> &getOCGs() const { return optionalContentGroups; }

    OptionalContentGroup *findOcgByRef(Ref ref) const;

    const Array *getOrderArray() const { return order.isArrayOfLengthAtLeast(1) ? order.getArray() : nullptr; }
    const Array *getRBGroupsArray() const { return rbgroups.isArrayOfLengthAtLeast(1) ? rbgroups.getArray() : nullptr; }

    // <budget> (DocSlurp): charged for what the evaluation visits; nullptr: no budget
    bool optContentIsVisible(const Object *dictRef, OCVisibilityBudget *budget = nullptr) const;

private:
    bool ok;

    // DocSlurp: the state of one evaluation (memoised indirect sub-expressions, operand budget).
    // A result cut short by the recursion limit (truncations) is never memoised: it depends on
    // the depth it was reached at. One that gave an error is memoised for this evaluation only.
    struct VisibilityEval
    {
        struct Result
        {
            bool visible;
            bool errored;
        };
        std::unordered_map<Ref, Result> memo;
        int nodes = 0;
        int truncations = 0;
        int errors = 0;
        bool overBudget = false;
    };
    bool evalOCVisibilityExpr(const Object *expr, int recursion, VisibilityEval &eval) const;
    bool evalVisibility(const Object *dictRef, long &cost, bool &cacheable, bool &limited) const;

    // DocSlurp: OCMD visibility by reference, across evaluations, until any group's state
    // changes (OptionalContentGroup::setState), at most cacheLimit entries: the answer, whether
    // it came from the expression limit (so the warning is given again), and the operands its
    // evaluation visited (charged to a page budget on every first use on a page). Only results
    // that gave no error and were not cut short are kept. /VE sub-expressions are not kept
    // across evaluations: an OCMD's cost and expression-limit answer would then depend on what
    // was evaluated before it.
    struct CachedVisibility
    {
        bool visible;
        bool limited;
        long cost;
    };
    static constexpr size_t cacheLimit = 1u << 16;
    void syncCache() const; // with cacheMutex held
    mutable std::mutex cacheMutex;
    mutable unsigned long cacheGeneration = 0;
    mutable std::unordered_map<Ref, CachedVisibility> ocmdCache;
    bool allOn(const Array &ocgArray) const;
    bool allOff(const Array &ocgArray) const;
    bool anyOn(const Array &ocgArray) const;
    bool anyOff(const Array &ocgArray) const;

    std::unordered_map<Ref, std::unique_ptr<OptionalContentGroup>> optionalContentGroups;

    Object order;
    Object rbgroups;
    XRef *m_xref;
};

//------------------------------------------------------------------------

class POPPLER_PRIVATE_EXPORT OptionalContentGroup
{
public:
    enum State
    {
        On,
        Off
    };

    // Values from the optional content usage dictionary.
    enum UsageState
    {
        ocUsageOn,
        ocUsageOff,
        ocUsageUnset
    };

    explicit OptionalContentGroup(Dict *dict);

    ~OptionalContentGroup();

    OptionalContentGroup(const OptionalContentGroup &) = delete;
    OptionalContentGroup &operator=(const OptionalContentGroup &) = delete;

    const GooString *getName() const;

    Ref getRef() const;
    void setRef(Ref ref);

    State getState() const { return m_state; };
    // DocSlurp: also drops every OCGs visibility cache (see OCGs::syncCache)
    void setState(State state);
    // DocSlurp: bumped by every setState
    static unsigned long stateGeneration();

    UsageState getViewState() const { return viewState; }
    UsageState getPrintState() const { return printState; }

private:
    std::unique_ptr<GooString> m_name;
    Ref m_ref;
    State m_state;
    UsageState viewState; // suggested state when viewing
    UsageState printState; // suggested state when printing
};

//------------------------------------------------------------------------

#endif
