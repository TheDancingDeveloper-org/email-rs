# Contributing to email-rs

## How changes land

Changes land through graft, batched. Workers `g land` on graft line `backlog-1`. Oversight cuts one batch PR from the line (`graft-mirror pr-batch`) and merges it with `land-pr --method rebase`. When a backlog of work is under way, all of it goes into a single batch PR. Batch rules: workspace `.claude/rules/delivery-process.md` ("Landing").

- Graft line: `backlog-1`; mirror: `graft-dev-mirror-email-rs`
- Required checks: `ci`, `runner-policy / runner-policy`, `release-dry-run`
- Merge queue: no
- Consumers (myaiagent) pin email-rs by release tag.
