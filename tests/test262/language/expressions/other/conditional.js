// Copyright (C) 2026 Megi contributors. All rights reserved.
// Locally authored Test262-style smoke case; see tests/test262/NOTICE.md.
/*---
esid: sec-conditional-operator
description: Conditional operator selects the matching branch.
flags: [noStrict]
---*/

assert.sameValue(true ? "yes" : "no", "yes", "true branch");
assert.sameValue(false ? "yes" : "no", "no", "false branch");
