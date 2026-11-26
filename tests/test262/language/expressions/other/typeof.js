// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-typeof-operator
description: typeof on primitive values.
flags: [noStrict]
---*/

assert.sameValue(typeof 1, "number", "typeof 1");
assert.sameValue(typeof "x", "string", "typeof 'x'");
