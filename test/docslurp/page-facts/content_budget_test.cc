// Exercise both limits against an ordinary PDF using a lowered test allowance.
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "TextOutputDev.h"

#include <memory>

class BudgetedTextOutputDev final : public TextOutputDev
{
public:
    BudgetedTextOutputDev() : TextOutputDev(nullptr, false, 0, false, false, false) { enableContentReadBudget(); }

    void startPage(int page, GfxState *state, XRef *xref) override
    {
        TextOutputDev::startPage(page, state, xref);
        if (lowerPageLimit) {
            getContentReadBudget()->pageRemaining = 1;
        }
    }

    bool lowerPageLimit = true;
};

int main(int argc, char **argv)
{
    if (argc != 2) {
        return 2;
    }
    globalParams = std::make_unique<GlobalParams>();
    globalParams->setErrQuiet(true);
    PDFDoc doc(std::make_unique<GooString>(argv[1]));
    if (!doc.isOk()) {
        return 2;
    }
    BudgetedTextOutputDev text;
    PageFacts facts(PageFacts::AllFamilies);
    text.setPageFacts(&facts);
    auto draw = [&] {
        facts.beginPage(1);
        doc.displayPage(&text, 1, 72, 72, 0, true, false, false);
        return facts.finishPage(1, 0);
    };
    auto *budget = text.getContentReadBudget();

    const auto incomplete = draw();
    if (incomplete.find("\"error\":\"failed\"") == std::string::npos || incomplete.find("\"geometry\"") != std::string::npos) {
        return 1;
    }
    if (!budget->truncated || budget->pageRemaining != 0 || budget->runRemaining != ContentReadBudget::runLimit - 1) {
        return 1;
    }
    text.lowerPageLimit = false;
    draw();
    if (budget->truncated || !text.contentWasTruncated()) {
        return 1;
    }

    budget->runRemaining = 1;
    draw();
    if (!budget->truncated || budget->runRemaining != 0 || budget->pageRemaining != ContentReadBudget::pageLimit - 1) {
        return 1;
    }
    draw();
    if (!budget->truncated || budget->pageRemaining != ContentReadBudget::pageLimit) {
        return 1;
    }
    return 0;
}
