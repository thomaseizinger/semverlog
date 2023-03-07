# semverlog - semantic versioning meets changelogs

The semantic versioning specification (https://semver.org) gives us a tool sanely evolve software.
However, as a library grows and perhaps gets split into multiple smaller ones, it can be difficult to keep track of the semver impact of a series of changes.

`semverlog` aims to solve this problem by allowing you to describe each change and its semver impact in a separate file under the `.changes` directory.

## Design philosophy

- **Zero configuration**: `semverlog` implements a very opinionated workflow.
  This allows it to operate without a configuration file, avoiding clutter in your repository.
- **Scalable**: With `semverlog`, each change (typically introduced in a single PR), is described in a separate file.
  This avoids merge conflicts in otherwise often touched files like `CHANGELOG.md` or manifest files.
- **Correct**: One of the biggest challenges in maintaining a large library is correctly tracking semver.
  With `semverlog`, you can record the semver impact of your change together with the code.
  This makes releases stress-free.
- **Flexible**: Despite being free of configuration files, `semverlog` aims to flexible where possible.
  For example, we don't integrate with package managers, instead `semverlog` requires you to pass in the current version of the package as an argument.

## Usage

1. Within your repository, create a directory named `.changes`.
2. Add a markdown file (name irrelevant) for each change (i.e. pull request).

Upon release time, call `semverlog compute-bump-level <CURRENT_VERSION>` to compute the next version bump.

To generate a changelog entry, call `semverlog compile-changelog <NEW_VERSION>`.
This will compile all entries within `.changes` together, sorted by priority and creation time.

## Change file format

A change file is a markdown file with a yaml front-matter:

```markdown
---
kind: added|fixed|changed|removed|deprecated|security
breaking: true|false
priority: 0-10
---
<CHANGE_TEXT>
```

`breaking` and `priority` are optional.

By default, the `changed` and `removed` kind are considered a breaking change, unless overridden with `breaking: false`.

`<CHANGE_TEXT>` will be output verbatim upon `semverlog compile-changelog`.
It is recommended to stick to a single paragraph and not add any extra headings to avoid formatting problems.

## Recommended workflow

- Squash-merge your PRs: This will ensure the change file is committed together with the actual change.
- Delete change files within `.changes` after each release.
  `semverlog` will always consider the entire `.changes` directory.
