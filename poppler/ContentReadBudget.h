// DocSlurp: bound decoded bytes read by the content lexer. GPLv2 or later.
#ifndef CONTENTREADBUDGET_H
#define CONTENTREADBUDGET_H

#include <algorithm>
#include <cstdint>

struct ContentReadBudget
{
    static constexpr uint64_t pageLimit = uint64_t(64) << 20;
    static constexpr uint64_t runLimit = uint64_t(2) << 30;

    void startPage()
    {
        pageRemaining = pageLimit;
        truncated = false;
        warned = false;
    }

    uint64_t remaining() const { return std::min(pageRemaining, runRemaining); }

    bool spend(uint64_t bytes = 1)
    {
        if (bytes > remaining()) {
            truncated = true;
            return false;
        }
        pageRemaining -= bytes;
        runRemaining -= bytes;
        return true;
    }

    uint64_t pageRemaining = pageLimit;
    uint64_t runRemaining = runLimit;
    bool truncated = false;
    bool warned = false;
};

#endif
