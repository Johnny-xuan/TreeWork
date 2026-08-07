# TreeWork for Pi

This adapter preserves TreeWork's existing protocol while mapping host-specific surfaces to Pi:

- the existing Agent Skill is loaded directly from `plugins/treework/skills/treework`;
- TreeWork's existing read-only MCP server backs Recall, Check, and Project Map tools;
- a small deferred loader keeps Pi's initial tool context minimal;
- explicit `/treework-enter` and `/treework-return` commands own cwd-bound
  session replacement, which Pi does not expose safely from an Agent tool;
- Pi `tool_call` hooks guard direct access to machine-owned TreeWork state;
- Pi `agent_settled` runs the existing `tw check --brief` boundary;
- Enter and Return fork the current Pi conversation into the target workspace so Pi rebuilds cwd-bound tools, project context, trust, settings, and resources correctly.

## Install

```bash
pi install git:github.com/Johnny-xuan/TreeWork
```

Restart Pi after installation. The package does not read or copy credentials.

To try a checkout without installing it:

```bash
pi -e /path/to/TreeWork/adapters/pi/index.ts \
  --skill /path/to/TreeWork/plugins/treework/skills/treework
```

## Use

Ask Pi to use TreeWork, or invoke `/skill:treework`. The adapter initially exposes only `treework_tools`; load `memory`, `map`, or `all` as needed.

When the Agent selects a branch, invoke:

```text
/treework-enter <branch>
```

This explicit host command waits until Pi is idle, requires a persisted session, prepares the target conversation fork before changing TreeWork state, performs the accepted Enter transaction, and switches into the managed Git worktree with full history. Explicit command ownership is necessary because Pi's Agent-tool context cannot safely replace its own cwd-bound session. If another extension cancels the switch, the adapter immediately pauses the entered branch and removes the unused fork.

After synchronizing and committing branch work, invoke `/treework-return` to move the conversation back to the same project's control workspace. Lifecycle transitions remain owned by `tw`; the adapter does not invent new state.

Project Map returns a loopback URL. It opens the system browser only when the caller explicitly sets `open: true`.

The tool hook blocks direct file-tool paths plus common explicit, split, globbed, and symlinked Bash paths into generated state. Like TreeWork's Codex hook, it is a cooperative Agent guardrail rather than a hostile-shell sandbox; do not try to obfuscate paths or bypass `tw` transactions.

## Roll back

```bash
pi remove git:github.com/Johnny-xuan/TreeWork
```

TreeWork project state and Git worktrees are not deleted by uninstalling the adapter. The Rust build cache is also retained so reinstalling does not rebuild it. To remove that optional cache after uninstalling:

```bash
rm -rf ~/.pi/agent/cache/treework
```

Set `PI_CODING_AGENT_DIR` accordingly if Pi uses a non-default agent directory.

## Verify

```bash
node --test adapters/pi/tests/*.test.mjs
python3 scripts/check_pi_adapter.py
python3 scripts/check_pi_workspace_switch.py
make test
make validate
```
