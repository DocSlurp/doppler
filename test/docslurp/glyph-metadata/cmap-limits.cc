#include "CharCodeToUnicode.h"
#include <cstdio>
#include <string>

int main()
{
    auto require = [](bool ok, const char *message) {
        if (!ok) {
            std::fprintf(stderr, "%s\n", message);
        }
        return ok;
    };
    size_t left = 2;
    auto map = CharCodeToUnicode::parseCMapLimited("1 beginbfchar <01> <0078> endbfchar", 8, left);
    const Unicode *value = nullptr;
    if (!require(map && map->mapToUnicode(1, &value) == 1 && value[0] == 'x' && left == 1, "valid map or accounting")) {
        return 1;
    }
    const std::string attacks[] = {
        "1 beginbfrange <0000> <ffff> <0041> endbfrange",
        "1 beginbfchar <ffffff> <0041> endbfchar",
        "/External usecmap",
        std::string(256 * 1024 + 1, ' '),
    };
    for (const auto &input : attacks) {
        left = 65536;
        if (!require(!CharCodeToUnicode::parseCMapLimited(input, 16, left), "unbounded map accepted")) {
            return 1;
        }
    }
    left = 1;
    if (!require(!CharCodeToUnicode::parseCMapLimited("2 beginbfchar <01> <0078> <02> <0079> endbfchar", 8, left) && left == 0, "shared budget bypass")) {
        return 1;
    }
    left = 0;
    if (!require(!CharCodeToUnicode::parseCMapLimited("1 beginbfchar <01> <0078> endbfchar", 8, left), "exhausted budget accepted")) {
        return 1;
    }
    return 0;
}
