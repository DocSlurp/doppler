// oc_state_probe: optional-content visibility follows group state changes, through OCGs'
// visibility caches (OCMD and /VE sub-expression results are kept until a state changes).
// usage: docslurp-oc-state-probe FILE.pdf
// Prints the visibility of page 1's /Properties /L as the document opens, with every group
// on, and with every group off again, as "0 1 0".
#include "Catalog.h"
#include "GlobalParams.h"
#include "OptionalContent.h"
#include "PDFDoc.h"
#include "Page.h"

#include <cstdio>
#include <memory>

int main(int argc, char **argv)
{
    if (argc != 2) {
        return 2;
    }
    globalParams = std::make_unique<GlobalParams>();
    globalParams->setErrQuiet(true);
    PDFDoc doc(std::make_unique<GooString>(argv[1]));
    const OCGs *ocgs = doc.isOk() ? doc.getCatalog()->getOptContentConfig() : nullptr;
    Page *page = ocgs ? doc.getPage(1) : nullptr;
    Dict *res = page ? page->getResourceDict() : nullptr;
    if (!res) {
        return 2;
    }
    const Object props = res->lookup("Properties");
    if (!props.isDict()) {
        return 2;
    }
    const Object &oc = props.getDict()->lookupNF("L");
    auto setAll = [ocgs](OptionalContentGroup::State s) {
        for (const auto &g : ocgs->getOCGs()) {
            g.second->setState(s);
        }
    };
    const bool v0 = ocgs->optContentIsVisible(&oc);
    // twice: the second answer comes from the cache
    const bool v0again = ocgs->optContentIsVisible(&oc);
    setAll(OptionalContentGroup::On);
    const bool v1 = ocgs->optContentIsVisible(&oc);
    setAll(OptionalContentGroup::Off);
    const bool v2 = ocgs->optContentIsVisible(&oc);
    printf("%d %d %d\n", v0 && v0again, v1, v2);
    return 0;
}
