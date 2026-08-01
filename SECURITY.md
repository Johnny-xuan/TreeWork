# Security Policy

## Supported Version

Security fixes are applied to the latest release line.

## Reporting a Vulnerability

Do not open a public issue for a vulnerability that could expose project files,
execute unintended commands, escape the local workspace, or make the Project
Map service reachable beyond loopback.

Use GitHub's private **Report a vulnerability** flow for this repository. Include
the affected version, operating system, reproduction steps, impact, and any
proposed mitigation.

TreeWork processes local project files, runs local hooks and MCP commands, and
serves a loopback web interface. Reports involving path traversal, command
execution, worktree deletion, transaction rollback, Host/Origin validation, or
state corruption are treated as security-sensitive.
