// Issue 45 (council): a writer that edits the document information and then adds an object, on
// a file whose cross-reference table lacks an entry, must keep its edits across the rebuild the
// missing entry makes. Usage: docslurp-info-edit-probe IN.pdf OUT.pdf
#include "PDFDoc.h"
#include "GlobalParams.h"
#include "XRef.h"
#include "goo/GooString.h"
#include <cstdio>
#include <memory>
int main(int argc, char **argv)
{
    if (argc != 3) {
        return 2;
    }
    globalParams = std::make_unique<GlobalParams>();
    auto doc = std::make_unique<PDFDoc>(std::make_unique<GooString>(argv[1]));
    if (!doc->isOk()) {
        return 1;
    }
    doc->setDocInfoStringEntry("Title", std::make_unique<GooString>("Record quarter"));
    doc->getXRef()->addIndirectObject(Object(42));
    return doc->saveAs(std::string(argv[2]), writeStandard) == errNone ? 0 : 1;
}
