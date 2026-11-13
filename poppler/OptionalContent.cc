//========================================================================
//
// OptionalContent.cc
//
// Copyright 2007 Brad Hards <bradh@kde.org>
// Copyright 2008 Pino Toscano <pino@kde.org>
// Copyright 2008, 2010 Carlos Garcia Campos <carlosgc@gnome.org>
// Copyright 2008, 2010, 2011, 2017-2019, 2025, 2026 Albert Astals Cid <aacid@kde.org>
// Copyright 2008 Mark Kaplan <mkaplan@finjan.com>
// Copyright 2018 Adam Reichold <adam.reichold@t-online.de>
// Copyright 2019 Oliver Sander <oliver.sander@tu-dresden.de>
// Copyright 2025 g10 Code GmbH, Author: Sune Stolborg Vuorela <sune@vuorela.dk>
// Copyright 2026 Adam Sampson <ats@offog.org>
//
// Released under the GPL (version 2, or later, at your option)
//
//========================================================================

#include <config.h>

#include "goo/GooString.h"
#include "Error.h"
#include "OptionalContent.h"
#include "XRef.h"

#include <atomic>

// Max depth of nested visibility expressions.  This is used to catch
// infinite loops in the visibility expression object structure.
constexpr int visibilityExprRecursionLimit = 50;
// DocSlurp: operands one visibility expression may visit (see evalOCVisibilityExpr).
constexpr int visibilityExprNodeLimit = 10000;

//------------------------------------------------------------------------

OCGs::OCGs(const Object &ocgObject, XRef *xref) : m_xref(xref)
{
    // we need to parse the dictionary here, and build optionalContentGroups
    ok = true;

    Object ocgs = ocgObject.dictLookup("OCGs");
    if (!ocgs.isArray()) {
        error(errSyntaxError, -1, "Expected the optional content group list, but wasn't able to find it, or it isn't an Array");
        ok = false;
        return;
    }
    Array *ocgsArray = ocgs.getArray();

    // we now enumerate over the ocgs list, and build up the optionalContentGroups list.
    for (int i = 0; i < ocgsArray->getLength(); ++i) {
        Object ocgDict = ocgsArray->get(i);
        if (!ocgDict.isDict()) {
            // skip bad entries (e.g. null) instead of dropping all later groups
            continue;
        }
        auto thisOptionalContentGroup = std::make_unique<OptionalContentGroup>(ocgDict.getDict());
        const Object &ocgRef = ocgsArray->getNF(i);
        if (!ocgRef.isRef()) {
            continue;
        }
        thisOptionalContentGroup->setRef(ocgRef.getRef());
        // the default is ON - we change state later, depending on BaseState, ON and OFF
        thisOptionalContentGroup->setState(OptionalContentGroup::On);
        optionalContentGroups.emplace(ocgRef.getRef(), std::move(thisOptionalContentGroup));
    }

    Object defaultOcgConfig = ocgObject.dictLookup("D");
    if (!defaultOcgConfig.isDict()) {
        error(errSyntaxError, -1, "Expected the default config, but wasn't able to find it, or it isn't a Dictionary");
        ok = false;
        return;
    }

    Object baseState = defaultOcgConfig.dictLookup("BaseState");
    if (baseState.isName("OFF")) {
        for (auto &group : optionalContentGroups) {
            group.second->setState(OptionalContentGroup::Off);
        }
    }

    Object on = defaultOcgConfig.dictLookup("ON");
    if (on.isArray()) {
        // ON is an optional element
        Array *onArray = on.getArray();
        for (int i = 0; i < onArray->getLength(); ++i) {
            const Object &reference = onArray->getNF(i);
            if (!reference.isRef()) {
                // there can be null entries
                continue;
            }
            OptionalContentGroup *group = findOcgByRef(reference.getRef());
            if (!group) {
                error(errSyntaxWarning, -1, "Couldn't find group for reference");
                continue;
            }
            group->setState(OptionalContentGroup::On);
        }
    }

    Object off = defaultOcgConfig.dictLookup("OFF");
    if (off.isArray()) {
        Array *offArray = off.getArray();
        // OFF is an optional element
        for (int i = 0; i < offArray->getLength(); ++i) {
            const Object &reference = offArray->getNF(i);
            if (!reference.isRef()) {
                // there can be null entries
                continue;
            }
            OptionalContentGroup *group = findOcgByRef(reference.getRef());
            if (!group) {
                error(errSyntaxWarning, -1, "Couldn't find group for reference to set OFF");
                continue;
            }
            group->setState(OptionalContentGroup::Off);
        }
    }

    order = defaultOcgConfig.dictLookup("Order");
    rbgroups = defaultOcgConfig.dictLookup("RBGroups");
}

bool OCGs::hasOCGs() const
{
    return !(optionalContentGroups.empty());
}

OptionalContentGroup *OCGs::findOcgByRef(const Ref ref) const
{
    const auto ocg = optionalContentGroups.find(ref);
    return ocg != optionalContentGroups.end() ? ocg->second.get() : nullptr;
}

// DocSlurp: every OptionalContentGroup::setState bumps this; an OCGs cache filled under an
// older value is dropped before its next use
static std::atomic<unsigned long> ocgStateGeneration { 0 };

void OCGs::syncCache() const
{
    const unsigned long g = OptionalContentGroup::stateGeneration();
    if (g != cacheGeneration) {
        ocmdCache.clear();
        cacheGeneration = g;
    }
}

// fixed texts (no arguments): page facts classify them as limit-hit from the format string
static const char *const exprTooLarge = "Optional content visibility expression too large; treated as visible";
static const char *const budgetSpent = "Optional content visibility budget for the page spent; treated as visible";

bool OCGs::optContentIsVisible(const Object *dictRef, OCVisibilityBudget *budget) const
{
    if (dictRef->isNull()) {
        return true;
    }

    if (dictRef->isRef()) {
        OptionalContentGroup *oc = findOcgByRef(dictRef->getRef());
        if (oc) {
            return oc->getState() == OptionalContentGroup::On;
        }
    }

    // DocSlurp: an OCMD named by many BDC /OC or XObject /OC entries is evaluated once (until a
    // group's state changes). Its first answer on a page is charged to the page's budget at the
    // cost of evaluating it, cached or not, so the page gets the same answers and limit-hit
    // whatever pages ran before it; later answers on the page are free, as in a cold run.
    if (budget && budget->generation != OptionalContentGroup::stateGeneration()) {
        budget->answered.clear();
        budget->generation = OptionalContentGroup::stateGeneration();
    }
    if (budget && dictRef->isRef()) {
        const auto seen = budget->answered.find(dictRef->getRef());
        if (seen != budget->answered.end()) {
            if (seen->second.limited) {
                error(errSyntaxWarning, -1, exprTooLarge);
                if (budget) {
                    budget->unevaluated = true;
                }
            }
            return seen->second.visible;
        }
    }
    std::lock_guard<std::mutex> lock(cacheMutex);
    syncCache();
    if (budget && budget->operandsLeft <= 0) {
        budget->unevaluated = true;
        if (!budget->limitHit) {
            budget->limitHit = true;
            error(errSyntaxWarning, -1, budgetSpent);
        }
        return true;
    }
    if (dictRef->isRef()) {
        const auto hit = ocmdCache.find(dictRef->getRef());
        if (hit != ocmdCache.end()) {
            const CachedVisibility cached = hit->second;
            if (cached.limited) {
                error(errSyntaxWarning, -1, exprTooLarge);
            }
            if (budget) {
                if (cached.limited) {
                    budget->unevaluated = true;
                }
                budget->operandsLeft -= cached.cost;
                budget->answered.emplace(dictRef->getRef(), OCVisibilityBudget::Answer { cached.visible, cached.limited });
            }
            return cached.visible;
        }
    }
    long cost = 1;
    bool cacheable = true, limited = false;
    const unsigned int suppressedBefore = m_xref ? m_xref->getSuppressedRepairs() : 0;
    const bool result = evalVisibility(dictRef, cost, cacheable, limited);
    if (m_xref && m_xref->getSuppressedRepairs() != suppressedBefore) {
        cacheable = false; // shaped by a repair a RepairGuard refused (issue 45)
    }
    if (dictRef->isRef() && cacheable) {
        if (budget) {
            budget->answered.emplace(dictRef->getRef(), OCVisibilityBudget::Answer { result, limited });
        }
        if (ocmdCache.size() < cacheLimit) {
            ocmdCache.emplace(dictRef->getRef(), CachedVisibility { result, limited, cost });
        }
    }
    if (budget) {
        budget->operandsLeft -= cost;
        if (limited) {
            budget->unevaluated = true;
        }
    }
    return result;
}

bool OCGs::evalVisibility(const Object *dictRef, long &cost, bool &cacheable, bool &limited) const
{
    Dict *dict;
    bool result = true;

    Object dictObj = dictRef->fetch(m_xref);
    if (!dictObj.isDict()) {
        error(errSyntaxWarning, -1, "Unexpected oc reference target: {0:d}", dictObj.getType());
        cacheable = false;
        return result;
    }
    dict = dictObj.getDict();
    Object dictType = dict->lookup("Type");
    if (dictType.isName("OCMD")) {
        Object ve = dict->lookup("VE");
        if (ve.isArray()) {
            // DocSlurp: one evaluation visits each indirect sub-expression once (an expression
            // whose levels share indirect arrays would otherwise take 2^depth steps) and at most
            // visibilityExprNodeLimit operands in all; past that the expression counts as visible
            VisibilityEval eval;
            result = evalOCVisibilityExpr(&ve, 0, eval);
            cost = eval.nodes > 1 ? eval.nodes : 1;
            if (eval.overBudget) {
                error(errSyntaxWarning, -1, exprTooLarge);
                result = true;
                limited = true;
            }
            if (eval.truncations > 0 || eval.errors > 0) {
                cacheable = false;
            }
        } else {
            const Object &ocg = dict->lookupNF("OCGs");
            if (ocg.isArray()) {
                cost = ocg.arrayGetLength() > 1 ? ocg.arrayGetLength() : 1;
                Object policy = dict->lookup("P");
                if (policy.isName("AllOn")) {
                    result = allOn(*ocg.getArray());
                } else if (policy.isName("AllOff")) {
                    result = allOff(*ocg.getArray());
                } else if (policy.isName("AnyOff")) {
                    result = anyOff(*ocg.getArray());
                } else if ((!policy.isName()) || (policy.isName("AnyOn"))) {
                    // this is the default
                    result = anyOn(*ocg.getArray());
                }
            } else if (ocg.isRef()) {
                OptionalContentGroup *oc = findOcgByRef(ocg.getRef());
                result = !oc || oc->getState() != OptionalContentGroup::Off;
            }
        }
    } else if (dictType.isName("OCG") && dictRef->isRef()) {
        OptionalContentGroup *oc = findOcgByRef(dictRef->getRef());
        if (oc && oc->getState() == OptionalContentGroup::Off) {
            result = false;
        }
    }
    return result;
}

bool OCGs::evalOCVisibilityExpr(const Object *expr, int recursion, VisibilityEval &eval) const
{
    OptionalContentGroup *ocg;
    bool ret;

    if (eval.overBudget || ++eval.nodes > visibilityExprNodeLimit) {
        eval.overBudget = true;
        return true;
    }
    if (recursion > visibilityExprRecursionLimit) {
        error(errSyntaxError, -1, "Loop detected in optional content visibility expression");
        // cut short: the answer depends on the depth, so nothing above it is memoised
        ++eval.truncations;
        return true;
    }
    if (expr->isRef()) {
        if ((ocg = findOcgByRef(expr->getRef()))) {
            return ocg->getState() == OptionalContentGroup::On;
        }
        const auto seen = eval.memo.find(expr->getRef());
        if (seen != eval.memo.end()) {
            if (seen->second.errored) {
                ++eval.errors;
            }
            return seen->second.visible;
        }
    }
    const int truncations = eval.truncations, errors = eval.errors;
    Object expr2 = expr->fetch(m_xref);
    if (!expr2.isArrayOfLengthAtLeast(1)) {
        error(errSyntaxError, -1, "Invalid optional content visibility expression");
        ++eval.errors;
        ret = true;
    } else {
        Array *expr2Array = expr2.getArray();
        Object op = expr2Array->get(0);
        if (op.isName("Not")) {
            if (expr2Array->getLength() == 2) {
                const Object &obj = expr2Array->getNF(1);
                ret = !evalOCVisibilityExpr(&obj, recursion + 1, eval);
            } else {
                error(errSyntaxError, -1, "Invalid optional content visibility expression");
                ++eval.errors;
                ret = true;
            }
        } else if (op.isName("And")) {
            ret = true;
            for (int i = 1; i < expr2Array->getLength() && ret && !eval.overBudget; ++i) {
                const Object &obj = expr2Array->getNF(i);
                ret = evalOCVisibilityExpr(&obj, recursion + 1, eval);
            }
        } else if (op.isName("Or")) {
            ret = false;
            for (int i = 1; i < expr2Array->getLength() && !ret && !eval.overBudget; ++i) {
                const Object &obj = expr2Array->getNF(i);
                ret = evalOCVisibilityExpr(&obj, recursion + 1, eval);
            }
        } else {
            error(errSyntaxError, -1, "Invalid optional content visibility expression");
            ++eval.errors;
            ret = true;
        }
    }
    if (expr->isRef() && !eval.overBudget && eval.truncations == truncations) {
        const bool errored = eval.errors != errors;
        eval.memo.emplace(expr->getRef(), VisibilityEval::Result { ret, errored });
    }
    return ret;
}

bool OCGs::allOn(const Array &ocgArray) const
{
    for (int i = 0; i < ocgArray.getLength(); ++i) {
        const Object &ocgItem = ocgArray.getNF(i);
        if (ocgItem.isRef()) {
            OptionalContentGroup *oc = findOcgByRef(ocgItem.getRef());
            if (oc && oc->getState() == OptionalContentGroup::Off) {
                return false;
            }
        }
    }
    return true;
}

bool OCGs::allOff(const Array &ocgArray) const
{
    for (int i = 0; i < ocgArray.getLength(); ++i) {
        const Object &ocgItem = ocgArray.getNF(i);
        if (ocgItem.isRef()) {
            OptionalContentGroup *oc = findOcgByRef(ocgItem.getRef());
            if (oc && oc->getState() == OptionalContentGroup::On) {
                return false;
            }
        }
    }
    return true;
}

bool OCGs::anyOn(const Array &ocgArray) const
{
    for (int i = 0; i < ocgArray.getLength(); ++i) {
        const Object &ocgItem = ocgArray.getNF(i);
        if (ocgItem.isRef()) {
            OptionalContentGroup *oc = findOcgByRef(ocgItem.getRef());
            if (oc && oc->getState() == OptionalContentGroup::On) {
                return true;
            }
        }
    }
    return false;
}

bool OCGs::anyOff(const Array &ocgArray) const
{
    for (int i = 0; i < ocgArray.getLength(); ++i) {
        const Object &ocgItem = ocgArray.getNF(i);
        if (ocgItem.isRef()) {
            OptionalContentGroup *oc = findOcgByRef(ocgItem.getRef());
            if (oc && oc->getState() == OptionalContentGroup::Off) {
                return true;
            }
        }
    }
    return false;
}

//------------------------------------------------------------------------

OptionalContentGroup::OptionalContentGroup(Dict *ocgDict) : m_name(nullptr)
{
    Object ocgName = ocgDict->lookup("Name");
    if (!ocgName.isString()) {
        error(errSyntaxWarning, -1, "Expected the name of the OCG, but wasn't able to find it, or it isn't a String");
    } else {
        m_name = ocgName.takeString();
    }

    viewState = printState = ocUsageUnset;
    Object obj1 = ocgDict->lookup("Usage");
    if (obj1.isDict()) {
        Object obj2 = obj1.dictLookup("View");
        if (obj2.isDict()) {
            Object obj3 = obj2.dictLookup("ViewState");
            if (obj3.isName()) {
                if (obj3.isName("ON")) {
                    viewState = ocUsageOn;
                } else {
                    viewState = ocUsageOff;
                }
            }
        }
        obj2 = obj1.dictLookup("Print");
        if (obj2.isDict()) {
            Object obj3 = obj2.dictLookup("PrintState");
            if (obj3.isName()) {
                if (obj3.isName("ON")) {
                    printState = ocUsageOn;
                } else {
                    printState = ocUsageOff;
                }
            }
        }
    }
}

const GooString *OptionalContentGroup::getName() const
{
    return m_name.get();
}

void OptionalContentGroup::setRef(const Ref ref)
{
    m_ref = ref;
}

Ref OptionalContentGroup::getRef() const
{
    return m_ref;
}

OptionalContentGroup::~OptionalContentGroup() = default;

void OptionalContentGroup::setState(State state)
{
    m_state = state;
    ++ocgStateGeneration;
}

unsigned long OptionalContentGroup::stateGeneration()
{
    return ocgStateGeneration.load();
}
