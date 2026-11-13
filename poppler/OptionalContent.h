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
// (Gfx keeps one per page). An evaluation not answered from the cache is charged the operands
// it visited (at least 1); once the budget is spent, such evaluations answer visible without
// looking, and the first one gives a warning (page facts: limit-hit).
struct OCVisibilityBudget
{
    static constexpr long perPage = 200000;
    long operandsLeft = perPage;
    bool limitHit = false;
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

    // DocSlurp: visibility by reference, across evaluations, until any group's state changes
    // (OptionalContentGroup::setState): OCMD dictionaries (with whether the answer came from a
    // budget, so the warning is given again) and /VE sub-expressions, each at most cacheLimit
    // entries. Only results that gave no error and were not cut short are kept.
    struct CachedVisibility
    {
        bool visible;
        bool limited;
    };
    static constexpr size_t cacheLimit = 1u << 16;
    void syncCache() const; // with cacheMutex held
    mutable std::mutex cacheMutex;
    mutable unsigned long cacheGeneration = 0;
    mutable std::unordered_map<Ref, CachedVisibility> ocmdCache;
    mutable std::unordered_map<Ref, bool> exprCache;
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
