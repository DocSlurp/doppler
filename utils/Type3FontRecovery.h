// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include "CharTypes.h"
#include <memory>

class Gfx8BitFont;
class XRef;

// A numeric glyph name proposes a slot. Verify the complete filled outline
// before consulting that slot in a locally installed Unicode face.
class Type3FontRecovery
{
public:
    Type3FontRecovery();
    ~Type3FontRecovery();
    Unicode lookup(Gfx8BitFont &font, unsigned code, XRef *xref);

private:
    struct State;
    std::unique_ptr<State> state;
};
