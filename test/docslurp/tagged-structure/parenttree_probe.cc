// Test helper: dumps what StructTreeRoot::findParentElement() returns for the
// first <count> MCIDs of ParentTree key <key>. With "--deref" it also reads
// the returned elements, which makes valgrind/ASan report a dangling pointer.
//
// usage: docslurp-parenttree-probe [--deref] file.pdf key count

#include "GlobalParams.h"
#include "PDFDoc.h"
#include "StructElement.h"
#include "StructTreeRoot.h"
#include "goo/GooString.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>

int main(int argc, char **argv)
{
    bool deref = false;
    int arg = 1;
    if (arg < argc && !strcmp(argv[arg], "--deref")) {
        deref = true;
        arg++;
    }
    if (argc - arg != 3) {
        fprintf(stderr, "usage: %s [--deref] file.pdf key count\n", argv[0]);
        return 2;
    }

    globalParams = std::make_unique<GlobalParams>();
    PDFDoc doc(std::make_unique<GooString>(argv[arg]));
    if (!doc.isOk()) {
        fprintf(stderr, "cannot open %s\n", argv[arg]);
        return 1;
    }
    const StructTreeRoot *root = doc.getStructTreeRoot();
    if (!root) {
        fprintf(stderr, "no structure tree\n");
        return 1;
    }

    const int key = atoi(argv[arg + 1]);
    const int count = atoi(argv[arg + 2]);
    for (int mcid = 0; mcid < count; mcid++) {
        const StructElement *element = root->findParentElement(key, mcid);
        if (!element) {
            printf("mcid %d: none\n", mcid);
        } else if (deref) {
            printf("mcid %d: %s children=%u\n", mcid, element->getTypeName(), element->getNumChildren());
        } else {
            printf("mcid %d: found\n", mcid);
        }
    }
    return 0;
}
