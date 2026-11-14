# Rules

1. Use short commit names and follow semantic commits. Do not add co-authors comments. Always work on main branch.
2. Whenever new keybinding is created/changed/removed, document it in KEYS.md
3. First commit and push, then run verifications and tests.
4. Maintain detailed file format specification in docs/spec/[FORMAT].md for all files that assetc produces.
5. Features of assetc should have spec first in docs/features/ as markdown.
6. Before a release, set the new version in every place it is written (listed in docs/features/release.md#version), check that none still has the old one, commit and push, and only then tag that commit `v<version>`.
