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
python3 scripts/package_plugin.py
python3 scripts/check_package_commit_source.py
```

The clean package appears at `dist/treework/`. It contains only the
tracked installable plugin subtree.

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

Create a GitHub release from the matching section of `RELEASE-NOTES.md`. Do not
move an existing release tag. Do not publish from a dirty worktree or from
uncommitted `dist/` contents.
