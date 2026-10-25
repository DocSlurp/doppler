// pdf2md_proto.cc -- PERFORMANCE PROBE, not a product. Quality is unvalidated.
// Purpose: measure what a single-pass Markdown projection of poppler's TextOutputDev tree costs
// relative to pdftotext, using ONLY public libpoppler API (no checkout edits).
//
//   pdf2md_proto [-f N] [-l N] [-j JOBS] [--mode md|null|text] [--tables 0|1] FILE.pdf   (Markdown on stdout)
//
// Design under test ("sample, then stream"):
//   1. sample <=6 evenly spaced pages -> body font size + heading size ladder  (global stats without a full pre-pass)
//   2. emit pages in order; with -j, N forked workers each own a PDFDoc + TextOutputDev for a contiguous page range
//      and write to a scratch file under $TMPDIR; the parent concatenates in page order.
// mode null  = interpret + coalesce, walk the tree, emit nothing (lower bound for any projection)
// mode text  = walk the tree, emit plain lines (no style/table/heading logic)
// mode md    = headings, bold/italic runs, links, pipe tables from aligned-cell lines
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <string>
#include <vector>
#include <map>
#include <algorithm>
#include <unistd.h>
#include <sys/wait.h>
#include "config.h"
#include "poppler-config.h"
#include "GlobalParams.h"
#include "PDFDoc.h"
#include "PDFDocFactory.h"
#include "TextOutputDev.h"
#include "Link.h"
#include "Annot.h"
#include "goo/GooString.h"
#include "GfxState.h"
#include "Object.h"

static std::unique_ptr<PDFDoc> openDoc(const char *path)
{
    auto d = PDFDocFactory().createPDFDoc(GooString(path), std::nullopt, std::nullopt);
    return d;
}

struct Stats
{
    double body = 10;
    std::vector<double> ladder; // heading sizes, largest first
};

struct WordView
{
    std::string text;
    double x0, x1, size;
    bool bold, italic, mono;
    std::string uri;
};

static bool nameHas(const GooString *n, const char *s)
{
    if (!n) {
        return false;
    }
    std::string a = n->toStr();
    for (auto &c : a) {
        c = tolower(c);
    }
    return a.find(s) != std::string::npos;
}

static WordView view(const TextWord *w)
{
    WordView v;
    auto t = w->getText();
    v.text = *t;
    double x0, y0, x1, y1;
    w->getBBox(&x0, &y0, &x1, &y1);
    v.x0 = x0;
    v.x1 = x1;
    v.size = w->getFontSize();
    const TextFontInfo *fi = w->getFontInfo(0);
    const GooString *fn = fi ? fi->getFontName() : nullptr;
    v.bold = fi && (fi->isBold() || nameHas(fn, "bold") || nameHas(fn, "black") || nameHas(fn, "heavy"));
    v.italic = fi && (fi->isItalic() || nameHas(fn, "italic") || nameHas(fn, "oblique"));
    v.mono = fi && (fi->isFixedWidth() || nameHas(fn, "mono") || nameHas(fn, "courier"));
    const AnnotLink *l = w->getLink();
    if (l && l->getAction() && l->getAction()->getKind() == actionURI) {
        v.uri = static_cast<LinkURI *>(l->getAction())->getURI();
    }
    return v;
}

static void walkPage(TextOutputDev &out, bool wantLinks, std::vector<std::vector<std::vector<WordView>>> &blocks)
{
    (void)wantLinks;
    for (const TextFlow *f = out.getFlows(); f; f = f->getNext()) {
        for (const TextBlock *b = f->getBlocks(); b; b = b->getNext()) {
            std::vector<std::vector<WordView>> lines;
            for (const TextLine *l = b->getLines(); l; l = l->getNext()) {
                std::vector<WordView> ws;
                for (const TextWord *w = l->getWords(); w; w = w->nextWord()) {
                    ws.push_back(view(w));
                }
                if (!ws.empty()) {
                    lines.push_back(std::move(ws));
                }
            }
            if (!lines.empty()) {
                blocks.push_back(std::move(lines));
            }
        }
    }
}

static void sampleStats(PDFDoc *doc, int first, int last, Stats &st)
{
    TextOutputDev out(nullptr, nullptr, false, 0, false, false);
    out.enableHTMLExtras(false);
    std::map<int, double> hist; // size*2 -> chars
    int n = last - first + 1, k = std::min(n, 6);
    for (int i = 0; i < k; ++i) {
        int p = first + (k == 1 ? 0 : i * (n - 1) / (k - 1));
        doc->displayPage(&out, p, 72, 72, 0, true, false, false);
        std::vector<std::vector<std::vector<WordView>>> bl;
        walkPage(out, false, bl);
        for (auto &b : bl) {
            for (auto &l : b) {
                for (auto &w : l) {
                    hist[(int)std::lround(w.size * 2)] += w.text.size();
                }
            }
        }
    }
    double best = 0;
    for (auto &[s, c] : hist) {
        if (c > best) {
            best = c;
            st.body = s / 2.0;
        }
    }
    for (auto it = hist.rbegin(); it != hist.rend(); ++it) {
        double s = it->first / 2.0;
        if (s >= st.body * 1.15 && (st.ladder.empty() || st.ladder.back() - s >= 1.0) && st.ladder.size() < 4) {
            st.ladder.push_back(s);
        }
    }
}

static std::string esc(const std::string &s)
{
    std::string o;
    o.reserve(s.size());
    for (char c : s) {
        if (c == '|' || c == '\\' || c == '`' || c == '*' || c == '_' || c == '[' || c == ']') {
            o += '\\';
        }
        o += c;
    }
    return o;
}

// render one line of words as inline markdown with style runs and links
static std::string inlineLine(const std::vector<WordView> &ws, size_t b, size_t e)
{
    std::string o;
    bool bold = false, ital = false;
    std::string uri;
    for (size_t i = b; i < e; ++i) {
        const WordView &w = ws[i];
        if (w.uri != uri) {
            if (!uri.empty()) {
                o += "](" + uri + ")";
            }
            if (!w.uri.empty()) {
                o += "[";
            }
            uri = w.uri;
        }
        if (w.bold != bold || w.italic != ital) {
            if (ital) { o += "*"; }
            if (bold) { o += "**"; }
            bold = w.bold;
            ital = w.italic;
            if (bold) { o += "**"; }
            if (ital) { o += "*"; }
        }
        o += esc(w.text);
        if (i + 1 < e) {
            o += ' ';
        }
    }
    if (ital) { o += "*"; }
    if (bold) { o += "**"; }
    if (!uri.empty()) {
        o += "](" + uri + ")";
    }
    return o;
}

// split a line into cells where the horizontal gap exceeds 1.2 em
static std::vector<std::pair<size_t, size_t>> cellsOf(const std::vector<WordView> &ws)
{
    std::vector<std::pair<size_t, size_t>> c;
    size_t s = 0;
    for (size_t i = 1; i < ws.size(); ++i) {
        if (ws[i].x0 - ws[i - 1].x1 > 1.2 * ws[i].size) {
            c.push_back({ s, i });
            s = i;
        }
    }
    c.push_back({ s, ws.size() });
    return c;
}

static void emitBlockMd(std::string &out, const std::vector<std::vector<WordView>> &lines, const Stats &st, bool tables)
{
    // table: >=3 consecutive lines with the same >=2 cell count and aligned first-word x (+-2pt) per column
    size_t i = 0;
    while (i < lines.size()) {
        if (tables) {
            auto c0 = cellsOf(lines[i]);
            size_t j = i + 1;
            if (c0.size() >= 2) {
                while (j < lines.size()) {
                    auto cj = cellsOf(lines[j]);
                    if (cj.size() != c0.size()) {
                        break;
                    }
                    bool al = true;
                    for (size_t k = 0; k < cj.size(); ++k) {
                        if (std::fabs(lines[j][cj[k].first].x0 - lines[i][c0[k].first].x0) > 0.5 * lines[i][0].size * (k + 1)) {
                            al = false;
                        }
                    }
                    if (!al) {
                        break;
                    }
                    ++j;
                }
                if (j - i >= 3) {
                    for (size_t r = i; r < j; ++r) {
                        auto cr = cellsOf(lines[r]);
                        out += "|";
                        for (auto &[b, e] : cr) {
                            out += " " + inlineLine(lines[r], b, e) + " |";
                        }
                        out += "\n";
                        if (r == i) {
                            out += "|";
                            for (size_t k = 0; k < cr.size(); ++k) {
                                out += " --- |";
                            }
                            out += "\n";
                        }
                    }
                    out += "\n";
                    i = j;
                    continue;
                }
            }
        }
        // paragraph / heading: gather lines until a table starts
        const auto &l = lines[i];
        double sz = 0;
        bool allBold = true;
        for (auto &w : l) {
            sz = std::max(sz, w.size);
            allBold = allBold && w.bold;
        }
        int level = 0;
        for (size_t k = 0; k < st.ladder.size(); ++k) {
            if (sz >= st.ladder[k] - 0.25) {
                level = k + 1;
                break;
            }
        }
        if (!level && allBold && l.size() < 14 && lines.size() == 1) {
            level = 4;
        }
        if (level) {
            out += std::string(level, '#') + " " + inlineLine(l, 0, l.size()) + "\n\n";
            ++i;
            continue;
        }
        std::string para;
        while (i < lines.size()) {
            if (tables && cellsOf(lines[i]).size() >= 2 && i + 2 < lines.size() && cellsOf(lines[i + 1]).size() == cellsOf(lines[i]).size()) {
                break;
            }
            std::string t = inlineLine(lines[i], 0, lines[i].size());
            if (!para.empty()) {
                if (para.back() == '-' && para.size() > 1 && isalpha((unsigned char)para[para.size() - 2])) {
                    para.pop_back();
                } else {
                    para += ' ';
                }
            }
            para += t;
            ++i;
        }
        if (!para.empty()) {
            out += para + "\n\n";
        }
    }
}

// "imgs" mode: TextOutputDev that also wants non-text content, to count image placements.
// Gfx::doImage calls str->getImageParams() BEFORE out->drawImage(), so merely returning true from needNonText()
// makes JPX images fully decode (poppler/Gfx.cc:4223, JPEG2000Stream.cc:161 -> init2 -> opj_decode at :392).
class ImgDev : public TextOutputDev
{
public:
    using TextOutputDev::TextOutputDev;
    bool needNonText() override { return true; }
    void drawImage(GfxState *, Object *, Stream *, int, int, GfxImageColorMap *, bool, const int *, bool) override { ++n; }
    void drawImageMask(GfxState *, Object *, Stream *, int, int, bool, bool, bool) override { ++n; }
    void drawSoftMaskedImage(GfxState *, Object *, Stream *, int, int, GfxImageColorMap *, bool, Stream *, int, int, GfxImageColorMap *, bool) override { ++n; }
    int n = 0;
};

static void renderRange(const char *path, int first, int last, const Stats &st, const std::string &mode, bool tables, FILE *dst)
{
    auto doc = openDoc(path);
    if (!doc->isOk()) {
        return;
    }
    if (mode == "imgs") {
        ImgDev id(nullptr, nullptr, false, 0, false, false);
        for (int p = first; p <= last; ++p) {
            doc->displayPage(&id, p, 72, 72, 0, true, false, false);
        }
        fprintf(stderr, "image callbacks=%d\n", id.n);
        return;
    }
    TextOutputDev out(nullptr, nullptr, false, 0, false, false);
    out.enableHTMLExtras(mode == "md"); // links + underline; costs a little
    size_t nwords = 0;
    for (int p = first; p <= last; ++p) {
        doc->displayPage(&out, p, 72, 72, 0, true, false, false);
        std::vector<std::vector<std::vector<WordView>>> bl;
        walkPage(out, mode == "md", bl);
        std::string buf;
        if (mode == "md") {
            buf += "<!-- page " + std::to_string(p) + " -->\n\n";
        }
        for (auto &b : bl) {
            for (auto &l : b) {
                nwords += l.size();
            }
            if (mode == "md") {
                emitBlockMd(buf, b, st, tables);
            } else if (mode == "text") {
                for (auto &l : b) {
                    buf += inlineLine(l, 0, l.size()) + "\n";
                }
                buf += "\n";
            }
        }
        if (dst) {
            fwrite(buf.data(), 1, buf.size(), dst);
        }
    }
    if (mode == "null") {
        fprintf(stderr, "words=%zu\n", nwords);
    }
}

int main(int argc, char **argv)
{
    int first = 1, last = 0, jobs = 1;
    std::string mode = "md";
    bool tables = true;
    const char *file = nullptr;
    for (int i = 1; i < argc; ++i) {
        if (!strcmp(argv[i], "-f")) first = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-l")) last = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-j")) jobs = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--mode")) mode = argv[++i];
        else if (!strcmp(argv[i], "--tables")) tables = atoi(argv[++i]);
        else file = argv[i];
    }
    if (!file) {
        fprintf(stderr, "usage\n");
        return 2;
    }
    globalParams = std::make_unique<GlobalParams>();
    globalParams->setErrQuiet(true);
    Stats st;
    {
        auto doc = openDoc(file);
        if (!doc->isOk()) {
            return 1;
        }
        if (last < 1 || last > doc->getNumPages()) last = doc->getNumPages();
        if (mode == "md") sampleStats(doc.get(), first, last, st);
    }
    int n = last - first + 1;
    if (jobs <= 1 || n < 2 * jobs) {
        renderRange(file, first, last, st, mode, tables, mode == "null" ? nullptr : stdout);
        return 0;
    }
    const char *td = getenv("TMPDIR");
    std::vector<std::string> files;
    std::vector<pid_t> pids;
    for (int j = 0; j < jobs; ++j) {
        int a = first + (long)n * j / jobs, b = first + (long)n * (j + 1) / jobs - 1;
        std::string fn = std::string(td ? td : ".") + "/pdf2md-w" + std::to_string(getpid()) + "-" + std::to_string(j);
        files.push_back(fn);
        pid_t pid = fork();
        if (pid == 0) {
            globalParams = std::make_unique<GlobalParams>();
            globalParams->setErrQuiet(true);
            FILE *f = fopen(fn.c_str(), "wb");
            renderRange(file, a, b, st, mode, tables, mode == "null" ? nullptr : f);
            fclose(f);
            _exit(0);
        }
        pids.push_back(pid);
    }
    for (auto p : pids) waitpid(p, nullptr, 0);
    for (auto &fn : files) {
        FILE *f = fopen(fn.c_str(), "rb");
        char buf[65536];
        size_t r;
        while (f && (r = fread(buf, 1, sizeof buf, f)) > 0) fwrite(buf, 1, r, stdout);
        if (f) fclose(f);
        unlink(fn.c_str());
    }
    return 0;
}
