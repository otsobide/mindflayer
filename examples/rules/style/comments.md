---
description: What a comment in code is for
paths:
  - "**/*.rs"
  - "**/*.{ts,tsx,js,jsx,vue}"
  - "**/*.py"
  - "**/*.go"
---

# Comments say why, not what

- A comment explains why the code is the way it is: the constraint it meets,
  the case it handles, the simpler alternative that was turned down and why.
  The code already says what it does.
- A doc comment says what a caller can rely on: what it returns, what it
  refuses, what it never does.
- Keep comments true. Change one with the code it describes, and delete one
  that no longer holds: a wrong comment costs more than a missing one.
- No commented-out code, since history keeps it; no `TODO` without the issue
  it waits on; and nothing addressed to a reviewer, or to the conversation
  that produced the change.
