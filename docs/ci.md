# Continuous integration and branch protection

GitHub Actions is the authoritative CI system for this repository. The workflow
files live in [`.github/workflows/`](../.github/workflows/).

## Required checks on `main`

GitHub's legacy branch-protection rule for `main` requires these status checks:

- `ci`
- `runner-policy / runner-policy`
- `release-dry-run`

The rule does not require branches to be up to date before merging. Force
pushes and branch deletion are disabled. It does not currently require signed
commits, linear history, approving reviews, conversation resolution, or
administrator enforcement.

The repository has no repository rulesets; protection is supplied by the
legacy branch-protection rule above. This was verified on 2026-11-02 with:

```bash
gh api repos/TheDancingDeveloper-org/email-rs/rulesets
gh api repos/TheDancingDeveloper-org/email-rs/branches/main/protection
```

These commands are read-only. Update this document whenever the GitHub settings
or required workflow job names change.

## What the checks cover

- `ci` is the aggregate result for Rust formatting, linting and tests, clean
  dependency checkout, Docker image build, and web/mobile frontend builds.
- `runner-policy / runner-policy` rejects workflow changes that violate the
  repository's runner policy.
- `release-dry-run` validates release construction without publishing a
  release. On pull requests whose changed paths do not affect release inputs,
  its aggregate job succeeds after recording that the expensive build jobs
  were skipped.

For the corresponding local commands, see
[Development and reproducible builds](development.md#required-checks).
