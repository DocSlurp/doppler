// Private evidence from the text extraction walk. Licensed under the GPLv2 or later.
#ifndef TAGGEDTEXTEVIDENCE_H
#define TAGGEDTEXTEVIDENCE_H

#include <set>
#include <string_view>
#include <utility>
#include <vector>

#include "Dict.h"

struct TaggedTextSpan
{
    int mcid = -1;
    int scope = -1; // 0 = page, positive = Form XObject invocation on this page
    Ref form = Ref::INVALID();
    bool pagination = false;
};

class TaggedTextEvidence
{
public:
    struct Frame
    {
        int scope = -1;
        Ref form = Ref::INVALID();
        unsigned token = 0;
        bool artifact = false;
    };

    struct Stream
    {
        Frame frame;
        size_t base = 0;
    };

    void startPage(int page)
    {
        startedPage = page;
        current = {};
        base = 0;
        nextScope = 0;
        events = 0;
        failed = false;
        pageAdmitted = chargeRun(); // reserve one page attribute, including fallback
        frames.clear();
        seen.clear();
        associations.clear();
    }

    // A page /Contents array is one stream for marked-content purposes.
    // Other Gfx streams (patterns, appearances, etc.) mask the parent context.
    Stream enterStream(bool page, Ref form)
    {
        Stream saved { current, base };
        base = frames.size();
        const int parentScope = current.scope;
        current = {};
        if (page) {
            current.scope = 0;
        } else if (parentScope >= 0 && form.num > 0 && charge()) {
            current.scope = ++nextScope;
            current.form = form;
            // An artifact can paint a Form. Keep that classification, while
            // MCIDs are local to the stream that declares them.
            if (saved.frame.artifact) {
                current.artifact = true;
                current.token = saved.frame.token;
            }
        }
        return saved;
    }

    void leaveStream(Stream saved)
    {
        if (frames.size() != base) {
            invalidate();
            frames.resize(base);
        }
        current = saved.frame;
        base = saved.base;
    }

    void begin(std::string_view tag, const Dict *properties, bool hasPropertyOperand)
    {
        if (!charge()) {
            return;
        }
        frames.push_back(current);
        if (current.scope < 0) {
            return;
        }
        if (tag == "Artifact") {
            current.artifact = true;
            current.token = properties && properties->lookupNF("Type").isName("Pagination") ? paginationToken : 0;
        } else if (!current.artifact && !properties && hasPropertyOperand && tag != "OC") {
            // An unresolved BDC operand could have overridden the enclosing MCID.
            current.token = 0;
        } else if (!current.artifact && properties) {
            // Read only direct operands from the property list already resolved by Gfx.
            const Object &id = properties->lookupNF("MCID");
            if (!id.isNull()) {
                current.token = 0;
                if (id.isInt() && id.getInt() >= 0) {
                    if (!seen.emplace(current.scope, id.getInt()).second) {
                        invalidate();
                        return;
                    }
                    associations.push_back({ id.getInt(), current.scope, current.form });
                    current.token = firstAssociation + static_cast<unsigned>(associations.size() - 1);
                }
            }
        }
    }

    void end()
    {
        if (failed) {
            return;
        }
        if (frames.size() == base) {
            invalidate();
            return;
        }
        current = frames.back();
        frames.pop_back();
    }

    unsigned glyph() { return current.token && charge() ? current.token : 0; }

    TaggedTextSpan span(unsigned token) const
    {
        if (token == paginationToken) {
            return { .pagination = true };
        }
        return token >= firstAssociation ? associations[token - firstAssociation] : TaggedTextSpan {};
    }

    void invalidate() { failed = true; }
    bool isValid() const { return !failed; }
    bool isPageAdmitted() const { return pageAdmitted; }
    bool wasStartedFor(int page) const { return startedPage == page; }
    bool isRunLimitHit() const { return runLimitHit; }

private:
    static constexpr unsigned paginationToken = 1;
    static constexpr unsigned firstAssociation = 2;
    static constexpr unsigned maxPageEvents = 16384;
    // Each admitted page or evidenced glyph can emit at most one attribute
    // group of fewer than 128 bytes. This also bounds total extra XML to
    // 8 MiB plus the single run-limit element.
    static constexpr unsigned maxRunEvents = 65536;

    bool charge()
    {
        if (failed || events == maxPageEvents) {
            invalidate();
            return false;
        }
        if (!chargeRun()) {
            return false;
        }
        ++events;
        return true;
    }

    bool chargeRun()
    {
        if (runEvents == maxRunEvents) {
            runLimitHit = true;
            invalidate();
            return false;
        }
        ++runEvents;
        return true;
    }

    Frame current;
    size_t base = 0;
    int nextScope = 0;
    int startedPage = 0;
    unsigned events = 0;
    unsigned runEvents = 0;
    bool runLimitHit = false;
    bool pageAdmitted = false;
    bool failed = false;
    std::vector<Frame> frames;
    std::set<std::pair<int, int>> seen;
    std::vector<TaggedTextSpan> associations;
};

#endif
