// Out-of-tree experiment (NOT part of poppler): single interpretation pass -> evidence JSONL.
// Links against SYSTEM libpoppler 26.10.0 (private headers from /usr/include/poppler). Demonstrates that
// TextOutputDev (words/lines/blocks/fonts/colors/links) + marked-content frames + path/image callbacks +
// StructTree walk can be joined without re-interpreting pages and without any Markdown knowledge.
#include <cstdio>
#include <cstdlib>
#include <cmath>
#include <string>
#include <vector>
#include <map>
#include <memory>
#include <chrono>
#include "poppler-config.h"
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "TextOutputDev.h"
#include "GfxState.h"
#include "Catalog.h"
#include "StructTreeRoot.h"
#include "StructElement.h"
#include "Link.h"
#include "Annot.h"
#include "Page.h"
#include "UTF.h"
#include "Dict.h"

static std::string esc(const std::string &s)
{
    std::string o;
    for (unsigned char c : s) {
        if (c == '"' || c == '\\') { o += '\\'; o += c; }
        else if (c < 0x20) { char b[8]; snprintf(b, sizeof b, "\\u%04x", c); o += b; }
        else o += c;
    }
    return o;
}
static void noop(void *, const char *, int) {}

struct Frame { std::string tag; int mcid; };
struct Rect { double x0, y0, x1, y1; };

class EvidenceDev : public TextOutputDev
{
public:
    EvidenceDev() : TextOutputDev(noop, nullptr, false, 0, false, false) { enableHTMLExtras(true); setInlineLinkURIs(true); }
    // ---- marked-content frames (public OutputDev callbacks only) ----
    void beginMarkedContent(const std::string &name, Dict *props) override {
        int id = -1;
        if (props) props->lookupInt("MCID", {}, &id);
        stack.push_back({name, id}); ++nBegin;
        TextOutputDev::beginMarkedContent(name, props);
    }
    void endMarkedContent(GfxState *s) override {
        ++nEnd; if (stack.empty()) ++underflow; else stack.pop_back();
        TextOutputDev::endMarkedContent(s);
    }
    void startPage(int p, GfxState *s, XRef *x) override {
        TextOutputDev::startPage(p, s, x); cp = 0; runs.clear(); rules.clear(); rects.clear(); images.clear(); stack.clear(); nBegin = nEnd = underflow = 0; lastMcid = -2;
    }
    int curMcid() const { for (auto it = stack.rbegin(); it != stack.rend(); ++it) if (it->mcid >= 0) return it->mcid; return -1; }
    bool curArtifact() const { for (auto &f : stack) if (f.tag == "Artifact") return true; return false; }
    // mirror TextPage::charPos so words can be joined to frames via TextWord::getCharPos()
    void drawChar(GfxState *s, double x, double y, double dx, double dy, double ox, double oy, CharCode c, int nBytes, const Unicode *u, int uLen) override {
        int m = curMcid(); int art = curArtifact();
        int key = m * 2 + art;
        if (runs.empty() || runs.back().key != key) runs.push_back({cp, key});
        TextOutputDev::drawChar(s, x, y, dx, dy, ox, oy, c, nBytes, u, uLen);
        cp += nBytes;
    }
    void incCharCount(int n) override { cp += n; TextOutputDev::incCharCount(n); }
    // text-only devices never see images/paths: Gfx.cc gates them on needNonText(). Toggle: env EV_NONTEXT=0 for the text-only cost baseline.
    bool needNonText() override { static const bool on = !getenv("EV_NONTEXT") || atoi(getenv("EV_NONTEXT")); return on; }
    // ---- vector shapes ----
    void fill(GfxState *s) override { path(s, true, false); TextOutputDev::fill(s); }
    void eoFill(GfxState *s) override { path(s, true, true); TextOutputDev::eoFill(s); }
    void stroke(GfxState *s) override { path(s, false, false); TextOutputDev::stroke(s); }
    void path(GfxState *s, bool filled, bool) {
        const GfxPath *p = s->getPath(); if (!p) return;
        GfxRGB rgb; if (filled) s->getFillRGB(&rgb); else s->getStrokeRGB(&rgb);
        for (int i = 0; i < p->getNumSubpaths(); ++i) {
            const GfxSubpath *sp = p->getSubpath(i);
            double x0 = 1e30, y0 = 1e30, x1 = -1e30, y1 = -1e30; int n = sp->getNumPoints();
            for (int j = 0; j < n; ++j) { double X, Y; s->transform(sp->getX(j), sp->getY(j), &X, &Y); x0 = std::min(x0, X); x1 = std::max(x1, X); y0 = std::min(y0, Y); y1 = std::max(y1, Y); }
            bool curves = false; for (int j = 0; j < n; ++j) if (sp->getCurve(j)) curves = true;
            Rect r{x0, y0, x1, y1};
            char col[16]; snprintf(col, sizeof col, "%02x%02x%02x", (int)(colToDbl(rgb.r) * 255 + .5), (int)(colToDbl(rgb.g) * 255 + .5), (int)(colToDbl(rgb.b) * 255 + .5));
            char b[256]; snprintf(b, sizeof b, "{\"k\":\"%s\",\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"n\":%d,\"curve\":%d,\"color\":\"%s\",\"lw\":%.2f,\"mc\":%d}",
                filled ? "fill" : "stroke", r.x0, r.y0, r.x1, r.y1, n, curves, col, s->getLineWidth(), curMcid());
            rules.push_back(b);
        }
    }
    // ---- images ----
    void imageRec(GfxState *s, Object *ref, int w, int h, const char *kind, bool inl) {
        double cx[4] = {0, 1, 1, 0}, cy[4] = {0, 0, 1, 1};
        double X0 = 1e30, Y0 = 1e30, X1 = -1e30, Y1 = -1e30;
        for (int i = 0; i < 4; ++i) { double X, Y; s->transform(cx[i], cy[i], &X, &Y); X0 = std::min(X0, X); X1 = std::max(X1, X); Y0 = std::min(Y0, Y); Y1 = std::max(Y1, Y); }
        char b[256]; snprintf(b, sizeof b, "{\"k\":\"%s\",\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"w\":%d,\"h\":%d,\"ref\":%d,\"inline\":%d,\"mc\":%d}", kind, X0, Y0, X1, Y1, w, h, (ref && ref->isRef()) ? ref->getRefNum() : -1, inl, curMcid());
        images.push_back(b);
    }
    void drawImageMask(GfxState *s, Object *r, Stream *st, int w, int h, bool inv, bool ip, bool inl) override { imageRec(s, r, w, h, "mask", inl); TextOutputDev::drawImageMask(s, r, st, w, h, inv, ip, inl); }
    void drawImage(GfxState *s, Object *r, Stream *st, int w, int h, GfxImageColorMap *cm, bool ip, const int *mk, bool inl) override { imageRec(s, r, w, h, "image", inl); TextOutputDev::drawImage(s, r, st, w, h, cm, ip, mk, inl); }
    void drawSoftMaskedImage(GfxState *s, Object *r, Stream *st, int w, int h, GfxImageColorMap *cm, bool ip, Stream *ms, int mw, int mh, GfxImageColorMap *mcm, bool mip) override { imageRec(s, r, w, h, "image+smask", false); TextOutputDev::drawSoftMaskedImage(s, r, st, w, h, cm, ip, ms, mw, mh, mcm, mip); }

    struct Run { int pos; int key; };
    std::vector<Frame> stack; std::vector<Run> runs; std::vector<std::string> rules, images;
    std::vector<Rect> rects; int cp = 0, nBegin = 0, nEnd = 0, underflow = 0, lastMcid;
    int mcidAt(int pos, int *art) const { int key = -2; for (auto &r : runs) { if (r.pos <= pos) key = r.key; else break; } if (key == -2) { *art = 0; return -1; } *art = key & 1; return (key - (key & 1)) / 2; }
};

static void walkStruct(PDFDoc *doc, const StructElement *e, std::vector<std::string> &path, int &row, int &col, FILE *o, int level)
{
    if (e->isContent()) {
        if (e->isObjectRef()) return;
        Ref pr; int pg = 0; if (e->getPageRef(pr)) pg = doc->findPage(pr);
        std::string p; for (auto &s : path) { if (!p.empty()) p += "/"; p += s; }
        fprintf(o, "{\"t\":\"struct\",\"page\":%d,\"mcid\":%d,\"path\":\"%s\",\"row\":%d,\"col\":%d}\n", pg, e->getMCID(), esc(p).c_str(), row, col);
        return;
    }
    std::string name = e->getTypeName();
    path.push_back(name);
    bool isTR = name == "TR", isCell = name == "TD" || name == "TH";
    if (isTR) { ++row; col = -1; }
    if (isCell) ++col;
    if (name == "Table") { row = -1; col = -1; }
    if (e->getAltText() || e->getActualText()) {
        std::string p; for (auto &s : path) { if (!p.empty()) p += "/"; p += s; }
        const GooString *a = e->getAltText();
        fprintf(o, "{\"t\":\"structmeta\",\"path\":\"%s\",\"alt\":\"%s\"}\n", esc(p).c_str(), a ? esc(TextStringToUtf8(a->toStr())).c_str() : "");
    }
    for (unsigned i = 0; i < e->getNumChildren(); ++i) walkStruct(doc, e->getChild(i), path, row, col, o, level + 1);
    path.pop_back();
}

int main(int argc, char **argv)
{
    if (argc < 2) return 2;
    globalParams = std::make_unique<GlobalParams>();
    auto doc = PDFDocFactory().createPDFDoc(GooString(argv[1]), std::nullopt, std::nullopt);
    if (!doc || !doc->isOk()) { fprintf(stderr, "open failed\n"); return 1; }
    int first = 1, last = doc->getNumPages(); if (argc > 3) { first = atoi(argv[2]); last = atoi(argv[3]); }
    FILE *o = stdout;
    auto t0 = std::chrono::steady_clock::now();
    fprintf(o, "{\"t\":\"doc\",\"pages\":%d,\"tagged\":%d,\"producer\":\"poppler %s\"}\n", doc->getNumPages(), doc->getCatalog()->getStructTreeRoot() != nullptr, POPPLER_VERSION);
    // metadata (one dict lookup each)
    for (const char *k : {"Title", "Author", "Subject", "Keywords", "Creator", "Producer", "CreationDate", "ModDate"}) {
        auto v = doc->getDocInfoStringEntry(k);
        if (v) fprintf(o, "{\"t\":\"meta\",\"k\":\"%s\",\"v\":\"%s\"}\n", k, esc(TextStringToUtf8(*v)).c_str());
    }
    if (StructTreeRoot *root = doc->getCatalog()->getStructTreeRoot()) {
        std::vector<std::string> path; int row = -1, col = -1;
        for (unsigned i = 0; i < root->getNumChildren(); ++i) walkStruct(doc.get(), root->getChild(i), path, row, col, o, 0);
    }
    EvidenceDev dev;
    for (int pg = first; pg <= last; ++pg) {
        doc->displayPage(&dev, pg, 72, 72, 0, true, false, false);
        fprintf(o, "{\"t\":\"page\",\"n\":%d,\"w\":%.2f,\"h\":%.2f,\"mcBegin\":%d,\"mcEnd\":%d,\"mcUnderflow\":%d}\n", pg, doc->getPageMediaWidth(pg), doc->getPageMediaHeight(pg), dev.nBegin, dev.nEnd, dev.underflow);
        int fi = 0, bi = 0;
        for (const TextFlow *f = dev.getFlows(); f; f = f->getNext(), ++fi)
            for (const TextBlock *b = f->getBlocks(); b; b = b->getNext(), ++bi) {
                double bx0, by0, bx1, by1; b->getBBox(&bx0, &by0, &bx1, &by1);
                fprintf(o, "{\"t\":\"block\",\"id\":%d,\"flow\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f}\n", bi, fi, bx0, by0, bx1, by1);
                int li = 0;
                for (const TextLine *l = b->getLines(); l; l = l->getNext(), ++li) {
                    PDFRectangle lb = l->getBBox();
                    fprintf(o, "{\"t\":\"line\",\"block\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"words\":[", bi, lb.x1, lb.y1, lb.x2, lb.y2);
                    bool firstw = true;
                    for (const TextWord *w = l->getWords(); w; w = w->getNext()) {
                        double x0, y0, x1, y1; w->getBBox(&x0, &y0, &x1, &y1); double r, g, bl; w->getColor(&r, &g, &bl);
                        const TextFontInfo *fi0 = w->getLength() ? w->getFontInfo(0) : nullptr;
                        std::string fname = fi0 && fi0->getFontName() ? fi0->getFontName()->toStr() : "";
                        std::string uri; if (const AnnotLink *al = w->getLink()) if (LinkAction *a = al->getAction()) if (a->getKind() == actionURI) uri = static_cast<LinkURI *>(a)->getURI();
                        int art = 0; int mc = dev.mcidAt(w->getCharPos(), &art);
                        auto txt = w->getText();
                        fprintf(o, "%s{\"s\":\"%s\",\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"base\":%.2f,\"fs\":%.2f,\"rot\":%d,\"font\":\"%s\",\"b\":%d,\"i\":%d,\"m\":%d,\"serif\":%d,\"u\":%d,\"sp\":%d,\"c\":\"%02x%02x%02x\",\"uri\":\"%s\",\"mc\":%d,\"art\":%d,\"cp\":%d}",
                            firstw ? "" : ",", esc(*txt).c_str(), x0, y0, x1, y1, w->getBaseline(), w->getFontSize(), w->getRotation(), esc(fname).c_str(),
                            fi0 ? fi0->isBold() : 0, fi0 ? fi0->isItalic() : 0, fi0 ? fi0->isFixedWidth() : 0, fi0 ? fi0->isSerif() : 0, w->isUnderlined(), w->getSpaceAfter(),
                            (int)(r * 255 + .5), (int)(g * 255 + .5), (int)(bl * 255 + .5), esc(uri).c_str(), mc, art, w->getCharPos());
                        firstw = false;
                    }
                    fprintf(o, "]}\n");
                }
            }
        for (auto &s : dev.rules) fprintf(o, "{\"t\":\"path\",\"page\":%d,\"d\":%s}\n", pg, s.c_str());
        for (auto &s : dev.images) fprintf(o, "{\"t\":\"image\",\"page\":%d,\"d\":%s}\n", pg, s.c_str());
        // links independent of word attachment: annotation rects (QuadPoints-accurate join is a future step)
        if (auto *annots = doc->getPage(pg)->getAnnots()) for (const auto &ap : annots->getAnnots()) if (ap->getType() == Annot::typeLink) {
            Annot *a = ap.get(); auto *al = static_cast<AnnotLink *>(a); std::string uri; if (LinkAction *act = al->getAction()) if (act->getKind() == actionURI) uri = static_cast<LinkURI *>(act)->getURI();
            const PDFRectangle *rc = &a->getRect(); fprintf(o, "{\"t\":\"link\",\"page\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"uri\":\"%s\"}\n", pg, rc->x1, rc->y1, rc->x2, rc->y2, esc(uri).c_str());
        }
    }
    auto t1 = std::chrono::steady_clock::now();
    fprintf(stderr, "probe_wall_s=%.3f\n", std::chrono::duration<double>(t1 - t0).count());
    return 0;
}
