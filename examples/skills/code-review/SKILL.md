---
name: code-review
description: Reviews a change for bugs, safety, tests and scope, most severe first. Use when asked to review a diff, a branch or a pull request.
---

# Code review

Review what changed against what it was meant to do, reading enough around
it to know what it touches. Not the whole codebase, and not the change you
would have written instead.

## Gather

1. The diff: `git diff <base>...HEAD` for a branch, `git diff --staged` for
   staged work, or the pull request's.
2. What it is for: the description, the issue, the commit messages.
3. The code around each hunk: the callers of a changed function, the tests
   beside it, the docs that describe it.

## Check, in this order

1. **Correctness.** Does it do what it says for every input? Try the edges:
   empty, one, many, missing, repeated, a path with spaces, a failure
   halfway through. Is every error handled or deliberately passed on, and is
   nothing left half-written when one happens?
2. **Safety.** Input from outside reaching a path, a shell, a query or a
   file name; a secret in code, logs or a URL; anything overwritten or
   deleted that the change did not create.
3. **Tests.** Would a test fail if the change were reverted? Are the edges
   above covered? Do the tests check what a user sees rather than how it is
   done?
4. **Scope.** Does the diff do only what it says? Reformatting, renames and
   fixes made along the way belong in commits of their own.
5. **Clarity.** Names say what things are, comments say why, and the docs
   and the README match the new behaviour.

## Report

Findings most severe first. For each: the file and line, what is wrong, a
concrete case that shows it (this input, that wrong result), and a fix.
Keep what blocks the change apart from suggestions and questions. Say
briefly what was checked and found sound, so the author knows what was
covered. When nothing is wrong, say so: a finding invented to have one costs
the author more than it saves.
