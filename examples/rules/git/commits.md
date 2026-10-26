# Commit one coherent step at a time

- Each commit builds and passes the tests on its own, so any one of them can
  be checked out, bisected or reverted alone.
- Write the subject as `<area>: <imperative summary>`; the commit-style skill
  has the rest.
- Stage files by path. `git add -A` and `git add .` take whatever else is
  lying in the tree: build output, scratch files, a secret. `git commit -a`
  takes every edit to a tracked file, including ones from another step.
- Never commit credentials, tokens or `.env` files. If one was committed, say
  so: deleting it in a later commit leaves it in the history.
- Never rewrite history somebody else has: no force-push to a shared branch,
  and `--force-with-lease` rather than `--force` on your own.
- Commit, push and open pull requests only when asked to.
