# Releasing

## Prepare

1. Update the version in:
   - `plugins/treework/.codex-plugin/plugin.json`
   - `plugins/treework/crates/treework-cli/Cargo.toml`
   - `plugins/treework/Cargo.lock`
   - `project-map-ui/package.json`
   - `project-map-ui/package-lock.json`
2. Update `RELEASE-NOTES.md`.
3. Build Project Map and review the generated asset diff.
4. Run `make test` and `make validate`.
5. Commit the complete release candidate.

## Package From the Commit

Packaging reads tracked bytes from `HEAD`, not the mutable worktree:

```bash
make package
```

Packaging produces three local forms:

- `dist/treework/` is the unpacked Coding Agents plugin candidate;
- `dist/releases/TreeWork-Coding-Agents-vX.Y.Z.zip` contains a self-contained
  local Codex marketplace rooted at `treework-coding-agents/`;
- `dist/releases/TreeWork-Manual-vX.Y.Z.zip` is the independently installable
  single-file Manual release asset rooted at `treework-manual/`.

Both ZIP assets come from the same committed `HEAD` and share the release tag,
but users install them independently.

## Install the Candidate

Register this checkout as a local marketplace when needed:

```bash
codex plugin marketplace add /absolute/path/to/treework
codex plugin add treework@treework
```

Start a new Codex task, then verify exact source, cache, prompt, hooks, and CLI
version:

```bash
python3 scripts/check_activation.py
```

## Publish

After the installed candidate passes:

```bash
git tag -a vX.Y.Z -m "TreeWork X.Y.Z"
git push origin main
git push origin vX.Y.Z
```

The tag workflow validates that `vX.Y.Z` matches the committed plugin version,
creates one GitHub Release, and uploads both edition ZIPs together. Do not move
an existing release tag. Do not publish from a dirty worktree or from
uncommitted `dist/` contents.
