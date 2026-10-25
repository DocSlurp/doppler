//========================================================================
//
// pngopt.h
//
// Parser for the -pngopt command line option shared by pdftoppm,
// pdftocairo and pdfimages.
//
// This file is licensed under the GPLv2 or later
//
//========================================================================

#ifndef PNGOPT_H
#define PNGOPT_H

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

#include "parseargs.h"

// Parses a string of the form <opt1>=<val1>[,<optN>=<valN>]*. The only
// option is "compression", an integer between 0 (fastest, largest files)
// and 9 (slowest, smallest files). On success *compression is set when the
// option is present and is left untouched otherwise. Returns false (after
// printing a message to stderr) on a malformed string.
inline bool parsePngOptions(const char *opts, int *compression)
{
    const char *nextOpt = opts;
    while (nextOpt && *nextOpt) {
        const char *comma = strchr(nextOpt, ',');
        std::string opt;
        if (comma) {
            opt.assign(nextOpt, comma - nextOpt);
            nextOpt = comma + 1;
        } else {
            opt.assign(nextOpt);
            nextOpt = nullptr;
        }
        const size_t equal = opt.find('=');
        if (equal == std::string::npos) {
            fprintf(stderr, "Unknown png option \"%s\"\n", opt.c_str());
            return false;
        }
        const std::string name = opt.substr(0, equal);
        const std::string value = opt.substr(equal + 1);

        if (name == "compression") {
            if (!isInt(value.c_str())) {
                fprintf(stderr, "Invalid png compression level\n");
                return false;
            }
            const long level = strtol(value.c_str(), nullptr, 10);
            if (level < 0 || level > 9) {
                fprintf(stderr, "png compression level must be between 0 and 9\n");
                return false;
            }
            *compression = static_cast<int>(level);
        } else {
            fprintf(stderr, "Unknown png option \"%s\"\n", opt.c_str());
            return false;
        }
    }
    return true;
}

#endif
