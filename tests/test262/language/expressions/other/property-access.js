// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-property-accessors
description: Dot and computed property access on an object literal.
flags: [noStrict]
---*/

var object = { a: 1, b: 2 };
assert.sameValue(object.a, 1, "dot access");
assert.sameValue(object["b"], 2, "computed access");
