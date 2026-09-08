# Repository rulesets

The two rulesets this repository runs under, kept here so they can be
re-applied verbatim and diffed like code.

- `protect-main.json` — the default branch can be neither force-pushed
  nor deleted.
- `protect-release-tags.json` — a release tag (`v[0-9]*`) is immutable
  once pushed: no update, no deletion. Creating tags stays free. The
  pattern is `v[0-9]*` rather than `v*` so a topical tag such as
  `vertical-alignment` is never caught.

These are friction and an audit trail, not cryptography: an account with
admin rights can delete a ruleset. Signed release tags remain the trust
anchor.

## Applying

GitHub enforces rulesets on private repositories only with a paid plan.
On the Free plan the API answers 403 until the repository is public, so
apply these at the moment the repository goes public:

```
gh api repos/jooize/Sajt/rulesets -X POST --input .github/rulesets/protect-main.json
gh api repos/jooize/Sajt/rulesets -X POST --input .github/rulesets/protect-release-tags.json
gh api repos/jooize/Sajt/rulesets --jq '.[] | {name,target,enforcement}'
```
