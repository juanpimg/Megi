// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-equality-operators
description: Strict equality of primitive values.
flags: [noStrict]
---*/

assert.sameValue(2 === 2, true, "2 === 2 is true");
assert.sameValue(2 === "2", false, "2 === '2' is false");
