// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-logical-and-operator
description: Logical AND returns the first falsy operand.
flags: [noStrict]
---*/

assert.sameValue(1 && 2, 2, "1 && 2 is 2");
assert.sameValue(0 && 2, 0, "0 && 2 is 0");
