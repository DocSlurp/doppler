// Experimental display-list recorder ("event IR") built OUT OF TREE against SYSTEM libpoppler 26.10.0
// headers (/usr/include/poppler). Not a tracked-checkout change. Emits JSONL: one record per
// text run / path / image / link. Modes: null (interpret only), text (needNonText=false), full.
#include <cstdio>
#include <cmath>
#include <string>
#include <memory>
#include <chrono>
#include "poppler-config.h"
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "Page.h"
#include "OutputDev.h"
#include "GfxState.h"
#include "GfxFont.h"
#include "Link.h"
#include "Annot.h"
#include "Catalog.h"
#include "UnicodeMap.h"
#include "UTF.h"

static void jesc(std::string &o, const std::string &s) {
    for (unsigned char c : s) {
        if (c == '"' || c == '\\') { o += '\\'; o += (char)c; }
        else if (c < 0x20) { char b[8]; snprintf(b, sizeof b, "\\u%04x", c); o += b; }
        else o += (char)c;
    }
}
static void utf8(std::string &o, Unicode u) {
    if (u < 0x80) o += (char)u;
    else if (u < 0x800) { o += (char)(0xC0 | u >> 6); o += (char)(0x80 | (u & 63)); }
    else if (u < 0x10000) { o += (char)(0xE0 | u >> 12); o += (char)(0x80 | ((u >> 6) & 63)); o += (char)(0x80 | (u & 63)); }
    else { o += (char)(0xF0 | u >> 18); o += (char)(0x80 | ((u >> 12) & 63)); o += (char)(0x80 | ((u >> 6) & 63)); o += (char)(0x80 | (u & 63)); }
}
struct Rec : OutputDev {
    enum Mode { NUL, TEXT, FULL } mode;
    FILE *out; int page = 0;
    long nText = 0, nChars = 0, nPath = 0, nRule = 0, nRect = 0, nImg = 0, nLink = 0;
    // current string run
    bool inRun = false; std::string run; double rx0, ry0, rx1, ry1, rfs; std::string rfont; bool rb, ri; int rrgb; double rrise;
    Rec(Mode m, FILE *f) : mode(m), out(f) {}
    bool upsideDown() override { return true; }
    bool useDrawChar() override { return true; }
    bool interpretType3Chars() override { return false; }
    bool needNonText() override { return mode == FULL; }
    void startPage(int n, GfxState *, XRef *) override { page = n; }
    static int rgb(GfxState *s, bool stroke = false) { GfxRGB c; if (stroke) s->getStrokeRGB(&c); else s->getFillRGB(&c); return (colToByte(c.r) << 16) | (colToByte(c.g) << 8) | colToByte(c.b); }
    bool colDirty = true; int cachedRgb = 0; bool fontDirty = true; std::string cachedFont; bool cachedB = false, cachedI = false;
#ifdef STYLE_CACHE
    void updateFillColor(GfxState *) override { colDirty = true; }
    void updateFillColorSpace(GfxState *) override { colDirty = true; }
    void updateFont(GfxState *) override { fontDirty = true; }
#endif
    void beginString(GfxState *, const std::string &) override { inRun = false; run.clear(); }
    void drawChar(GfxState *s, double x, double y, double dx, double dy, double, double, CharCode, int, const Unicode *u, int uLen) override {
        if (mode == NUL) return;
        nChars++;
        // FIX (coordinator review): drawChar x,y,dx,dy are USER space (TextPage::addChar calls state->transform itself); convert to device.
        double ux = x, uy = y; s->transform(ux, uy, &x, &y); { double ddx, ddy; s->transformDelta(dx, dy, &ddx, &ddy); dx = ddx; dy = ddy; }
        if (!inRun) {
            inRun = true; rx0 = x; ry0 = y; rfs = s->getTransformedFontSize(); rrise = s->getRise();
#ifdef STYLE_CACHE
            if (colDirty) { cachedRgb = rgb(s); colDirty = false; }
            if (fontDirty) { const auto &f = s->getFont(); cachedFont = f && f->getName() ? f->getNameWithoutSubsetTag() : ""; cachedB = f && f->isBold(); cachedI = f && f->isItalic(); fontDirty = false; }
            rrgb = cachedRgb; rfont = cachedFont; rb = cachedB; ri = cachedI;
#else
            rrgb = rgb(s);
            const auto &f = s->getFont(); rfont = f && f->getName() ? f->getNameWithoutSubsetTag() : "";
            rb = f && f->isBold(); ri = f && f->isItalic();
#endif
        }
        rx1 = x + dx; ry1 = y + dy;
        for (int i = 0; i < uLen; i++) utf8(run, u[i]);
    }
    void endString(GfxState *) override {
        if (mode == NUL || !inRun) return;
        nText++;
#ifdef NOEMIT
        inRun = false; return;
#endif
        std::string j, e; jesc(e, run); std::string fn; jesc(fn, rfont);
        char b[512];
        snprintf(b, sizeof b, "{\"t\":\"txt\",\"p\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"fs\":%.2f,\"rise\":%.2f,\"b\":%d,\"i\":%d,\"c\":\"%06x\",\"f\":\"", page, rx0, ry0, rx1, ry1, rfs, rrise, rb, ri, rrgb);
        fputs(b, out); fputs(fn.c_str(), out); fputs("\",\"s\":\"", out); fputs(e.c_str(), out); fputs("\"}\n", out);
        inRun = false;
    }
    void pathBox(GfxState *s, const char *kind) {
        if (mode != FULL) return;
        const GfxPath *p = s->getPath(); if (!p) return;
        nPath++;
        for (int i = 0; i < p->getNumSubpaths(); i++) {
            const GfxSubpath *sp = p->getSubpath(i); int n = sp->getNumPoints(); if (n < 2) continue;
            double x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9; bool curve = false, axis = true;
            double px = 0, py = 0;
            for (int k = 0; k < n; k++) {
                double dx, dy; s->transform(sp->getX(k), sp->getY(k), &dx, &dy);
                if (sp->getCurve(k)) curve = true;
                if (k && fabs(dx - px) > 0.01 && fabs(dy - py) > 0.01) axis = false;
                px = dx; py = dy;
                x0 = fmin(x0, dx); y0 = fmin(y0, dy); x1 = fmax(x1, dx); y1 = fmax(y1, dy);
            }
            const char *cls = "poly";
            double w = x1 - x0, h = y1 - y0;
            if (!curve && axis && n == 2) cls = "line";
            else if (!curve && axis && (n == 4 || n == 5)) cls = (fmin(w, h) <= 1.5) ? "rule" : "rect";
            else if (curve) cls = "curve";
            if (!strcmp(cls, "rule") || !strcmp(cls, "line")) nRule++; else if (!strcmp(cls, "rect")) nRect++;
            fprintf(out, "{\"t\":\"%s\",\"cls\":\"%s\",\"p\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"n\":%d,\"lw\":%.2f,\"c\":\"%06x\"}\n", kind, cls, page, x0, y0, x1, y1, n, s->getLineWidth() * 1.0, rgb(s, kind[0] == 's'));
        }
    }
    void fill(GfxState *s) override { pathBox(s, "fill"); }
    void eoFill(GfxState *s) override { pathBox(s, "fill"); }
    void stroke(GfxState *s) override { pathBox(s, "stroke"); }
    void imgRec(GfxState *s, int w, int h, const char *kind, Object *ref) {
        if (mode != FULL) return; nImg++;
        double x[4], y[4]; const double cx[4] = {0, 1, 0, 1}, cy[4] = {0, 0, 1, 1};
        double x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9;
        for (int k = 0; k < 4; k++) { s->transform(cx[k], cy[k], &x[k], &y[k]); x0 = fmin(x0, x[k]); x1 = fmax(x1, x[k]); y0 = fmin(y0, y[k]); y1 = fmax(y1, y[k]); }
        int num = ref && ref->isRef() ? ref->getRefNum() : -1;
        fprintf(out, "{\"t\":\"img\",\"kind\":\"%s\",\"p\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"pw\":%d,\"ph\":%d,\"obj\":%d}\n", kind, page, x0, y0, x1, y1, w, h, num);
    }
    void drawImage(GfxState *s, Object *ref, Stream *, int w, int h, GfxImageColorMap *, bool, const int *, bool) override { imgRec(s, w, h, "image", ref); }
    void drawImageMask(GfxState *s, Object *ref, Stream *, int w, int h, bool, bool, bool) override { imgRec(s, w, h, "mask", ref); }
    void drawMaskedImage(GfxState *s, Object *ref, Stream *, int w, int h, GfxImageColorMap *, bool, Stream *, int, int, bool, bool) override { imgRec(s, w, h, "masked", ref); }
    void drawSoftMaskedImage(GfxState *s, Object *ref, Stream *, int w, int h, GfxImageColorMap *, bool, Stream *, int, int, GfxImageColorMap *, bool) override { imgRec(s, w, h, "softmasked", ref); }
};
int main(int argc, char **argv) {
    if (argc < 3) { fprintf(stderr, "usage: recorder null|text|full in.pdf [out.jsonl]\n"); return 2; }
    Rec::Mode m = !strcmp(argv[1], "null") ? Rec::NUL : !strcmp(argv[1], "text") ? Rec::TEXT : Rec::FULL;
    globalParams = std::make_unique<GlobalParams>();
    auto t0 = std::chrono::steady_clock::now();
    PDFDoc doc(std::make_unique<GooString>(argv[2]));
    if (!doc.isOk()) { fprintf(stderr, "open failed\n"); return 1; }
    FILE *f = argc > 3 ? fopen(argv[3], "w") : fopen("/dev/null", "w");
    Rec r(m, f);
    int np = doc.getNumPages();
    for (int p = 1; p <= np; p++) {
        Page *pg = doc.getPage(p); if (!pg) continue;
        { bool sw = pg->getRotate() % 180 != 0;
          fprintf(f, "{\"t\":\"page\",\"p\":%d,\"w\":%.2f,\"h\":%.2f,\"rot\":%d}\n", p, sw ? pg->getMediaHeight() : pg->getMediaWidth(), sw ? pg->getMediaWidth() : pg->getMediaHeight(), pg->getRotate()); }
        doc.displayPage(&r, p, 72, 72, 0, true, false, false);
        if (m == Rec::FULL) {
            auto links = pg->getLinks();
            for (const auto &l : links->getLinks()) {
                double x1, y1, x2, y2; l->getRect(&x1, &y1, &x2, &y2);
                const LinkAction *a = l->getAction(); std::string u;
                if (a && a->getKind() == actionURI) u = static_cast<const LinkURI *>(a)->getURI();
                else if (a && a->getKind() == actionGoTo) u = "#goto";
                std::string e; jesc(e, u); r.nLink++;
                fprintf(f, "{\"t\":\"link\",\"p\":%d,\"x0\":%.2f,\"y0\":%.2f,\"x1\":%.2f,\"y1\":%.2f,\"dst\":\"%s\"}\n", p, x1, pg->getMediaHeight() - y2, x2, pg->getMediaHeight() - y1, e.c_str());
            }
        }
    }
    fclose(f);
    double s = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
    fprintf(stderr, "mode=%s pages=%d wall=%.3fs text=%ld chars=%ld paths=%ld rules=%ld rects=%ld imgs=%ld links=%ld\n", argv[1], np, s, r.nText, r.nChars, r.nPath, r.nRule, r.nRect, r.nImg, r.nLink);
}
