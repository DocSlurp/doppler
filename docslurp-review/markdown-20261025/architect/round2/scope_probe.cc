// Round-2 out-of-tree probe (system libpoppler 26.10.0, private headers). Shows what scope key the content-side join needs.
// Naive key = MCID (round-1 probe). Scoped key = (form ref num or 0 for page stream, form invocation serial, MCID).
// Words are joined via TextWord::getCharPos() against runs recorded in a charPos mirror; lookup is binary search (upper_bound).
#include <cstdio>
#include <algorithm>
#include <memory>
#include <string>
#include <vector>
#include "poppler-config.h"
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "TextOutputDev.h"
#include "Dict.h"
static void noop(void *, const char *, int) {}
struct Key { int formRef, inv, mcid, artifact, actualText; };
struct Frame { std::string tag; int mcid; bool at; };
class Dev : public TextOutputDev {
public:
    Dev() : TextOutputDev(noop, nullptr, false, 0, false, false) {}
    struct Scope { int ref, inv; };
    std::vector<Scope> forms; int invSerial = 0;
    std::vector<Frame> st; struct Run { int pos; Key k; }; std::vector<Run> runs; int cp = 0; int begins = 0, ends = 0, atBytesInSpan = 0;
    void beginForm(Object *o, Ref id) override { forms.push_back({id.num, ++invSerial}); TextOutputDev::beginForm(o, id); }
    void endForm(Object *o, Ref id) override { forms.pop_back(); TextOutputDev::endForm(o, id); }
    void beginMarkedContent(const std::string &n, Dict *p) override { int id = -1; if (p) p->lookupInt("MCID", {}, &id); st.push_back({n, id, false}); ++begins; TextOutputDev::beginMarkedContent(n, p); }
    void endMarkedContent(GfxState *s) override { ++ends; if (!st.empty()) st.pop_back(); TextOutputDev::endMarkedContent(s); }
    Key cur() const { Key k{forms.empty() ? 0 : forms.back().ref, forms.empty() ? 0 : forms.back().inv, -1, 0, 0}; for (auto it = st.rbegin(); it != st.rend(); ++it) if (it->mcid >= 0) { k.mcid = it->mcid; break; } return k; }
    void drawChar(GfxState *s, double x, double y, double dx, double dy, double ox, double oy, CharCode c, int nBytes, const Unicode *u, int uLen) override {
        Key k = cur(); if (runs.empty() || runs.back().k.formRef != k.formRef || runs.back().k.inv != k.inv || runs.back().k.mcid != k.mcid) runs.push_back({cp, k});
        TextOutputDev::drawChar(s, x, y, dx, dy, ox, oy, c, nBytes, u, uLen); cp += nBytes;
    }
    void incCharCount(int n) override { cp += n; TextOutputDev::incCharCount(n); }
    const Key *at(int pos) const { auto it = std::upper_bound(runs.begin(), runs.end(), pos, [](int p, const Run &r) { return p < r.pos; }); return it == runs.begin() ? nullptr : &std::prev(it)->k; }
};
int main(int argc, char **argv) {
    globalParams = std::make_unique<GlobalParams>();
    auto doc = PDFDocFactory().createPDFDoc(GooString(argv[1]), std::nullopt, std::nullopt);
    if (!doc || !doc->isOk()) return 1;
    bool stats = argc > 2; Dev dev; long W = 0, straddle = 0, inForm = 0, noMc = 0, ends = 0, begins = 0, under = 0;
    for (int pg = 1; pg <= doc->getNumPages() && (stats || pg == 1); ++pg) {
        dev.runs.clear(); dev.st.clear(); dev.cp = 0; dev.begins = dev.ends = 0; doc->displayPage(&dev, pg, 72, 72, 0, true, false, false);
        begins += dev.begins; ends += dev.ends; if (dev.ends > dev.begins) ++under;
        if (!stats) break;
        for (const TextFlow *f = dev.getFlows(); f; f = f->getNext()) for (const TextBlock *b = f->getBlocks(); b; b = b->getNext()) for (const TextLine *l = b->getLines(); l; l = l->getNext()) for (const TextWord *w = l->getWords(); w; w = w->getNext()) {
            ++W; const Key *k = dev.at(w->getCharPos()); if (!k || k->mcid < 0) ++noMc; if (k && k->formRef) ++inForm;
            const Key *k2 = dev.at(w->getCharPos() + w->getCharLen()); if (k && k2 && (k->mcid != k2->mcid || k->formRef != k2->formRef || k->inv != k2->inv)) ++straddle;
        }
    }
    if (stats) { printf("{\"words\":%ld,\"noMcid\":%ld,\"inForm\":%ld,\"firstLastCharKeyDiffers\":%ld,\"pagesWithEndWithoutBegin\":%ld,\"begins\":%ld,\"ends\":%ld}\n", W, noMc, inForm, straddle, under, begins, ends); return 0; }
    printf("begins=%d ends=%d\n%-8s %5s %-22s %s\n", dev.begins, dev.ends, "word", "cp", "naive(mcid)", "scoped(formRef,invocation,mcid)");
    for (const TextFlow *f = dev.getFlows(); f; f = f->getNext()) for (const TextBlock *b = f->getBlocks(); b; b = b->getNext()) for (const TextLine *l = b->getLines(); l; l = l->getNext()) for (const TextWord *w = l->getWords(); w; w = w->getNext()) {
        const Key *k = dev.at(w->getCharPos()); auto t = w->getText();
        printf("%-8s %5d mcid=%-17d form=%d inv=%d mcid=%d\n", t->c_str(), w->getCharPos(), k ? k->mcid : -9, k ? k->formRef : -9, k ? k->inv : -9, k ? k->mcid : -9);
    }
}
