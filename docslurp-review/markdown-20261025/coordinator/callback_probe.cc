// Isolated review probe. Linked against installed Poppler, not the checkout.
#include <PDFDoc.h>
#include <GlobalParams.h>
#include <OutputDev.h>
#include <MarkedContentOutputDev.h>
#include <goo/GooString.h>
#include <chrono>
#include <iostream>
#include <string>

class Probe final : public OutputDev {
public:
    bool opaque;
    long chars = 0, fills = 0, patterns = 0, begins = 0, ends = 0;
    explicit Probe(bool opaque) : opaque(opaque) {}
    bool upsideDown() override { return true; }
    bool useDrawChar() override { return true; }
    bool interpretType3Chars() override { return false; }
    bool useTilingPatternFill() override { return opaque; }
    bool tilingPatternFill(GfxState *, Gfx *, Catalog *, GfxTilingPattern *, const std::array<double,6> &, int,int,int,int,double,double) override { ++patterns; return true; }
    void fill(GfxState *) override { ++fills; }
    void drawChar(GfxState *, double,double,double,double,double,double,CharCode,int,const Unicode *,int) override { ++chars; }
    void beginMarkedContent(const std::string &name, Dict *props) override {
        ++begins; int mcid=-1;
        if (props) props->lookupInt("MCID", {}, &mcid);
        std::cout << "begin " << name << " mcid=" << mcid << '\n';
    }
    void endMarkedContent(GfxState *) override { ++ends; std::cout << "end\n"; }
};
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    globalParams = std::make_unique<GlobalParams>();
    PDFDoc doc(std::make_unique<GooString>(argv[1]));
    if (!doc.isOk()) return 1;
    if (argc > 2 && std::string(argv[2]) == "marked") {
        MarkedContentOutputDev marked(7, Object());
        doc.displayPage(&marked,1,72,72,0,false,true,false);
        std::cout << "MCID7 text=";
        for (const auto &span : marked.getTextSpans()) std::cout << span.getText()->toStr();
        std::cout << '\n';
        return 0;
    }
    Probe p(argc > 2);
    auto start=std::chrono::steady_clock::now();
    doc.displayPage(&p,1,72,72,0,false,true,false);
    std::cout << "chars=" << p.chars << " fills=" << p.fills << " patterns=" << p.patterns << " begins=" << p.begins << " ends=" << p.ends
              << " elapsed_us=" << std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now()-start).count() << '\n';
}
