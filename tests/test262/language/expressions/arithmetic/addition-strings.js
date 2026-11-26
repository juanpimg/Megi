// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-addition-operator-plus
description: String concatenation with the addition operator.
flags: [noStrict]
---*/

assert.sameValue("a" + "b", "ab", "'a' + 'b' is 'ab'");
