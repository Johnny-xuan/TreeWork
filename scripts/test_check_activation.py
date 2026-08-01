#!/usr/bin/env python3
"""Unit tests for candidate-bound activation attestation."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

import check_activation


class ActivationAttestationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(
            prefix="treework-activation-attestation-"
        )
        root = Path(self.temporary.name)
        self.repository = root / "repository"
        self.code_home = root / "codex-home"
        self.source = self.repository / "plugins" / check_activation.PLUGIN_NAME
        self.version = "0.1.0+fixture"
        (self.repository / ".agents" / "plugins").mkdir(parents=True)
        (self.code_home / "plugins" / "cache").mkdir(parents=True)
        (self.source / ".codex-plugin").mkdir(parents=True)
        (self.source / "skills" / "treework").mkdir(parents=True)
        (self.source / "skills" / "treework" / "scripts").mkdir()
        (self.source / "hooks").mkdir(parents=True)
        (self.repository / ".agents" / "plugins" / "marketplace.json").write_text(
            json.dumps(
                {
                    "name": check_activation.MARKETPLACE_NAME,
                    "plugins": [
                        {
                            "name": check_activation.PLUGIN_NAME,
                            "source": {
                                "source": "local",
                                "path": "./plugins/treework",
                            },
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        (self.code_home / "config.toml").write_text(
            '[marketplaces.treework]\n'
            'source = "fixture"\n'
            '[plugins."treework@treework"]\n'
            "enabled = true\n",
            encoding="utf-8",
        )
        (self.source / ".codex-plugin" / "plugin.json").write_text(
            json.dumps(
                {
                    "name": check_activation.PLUGIN_NAME,
                    "version": self.version,
                }
            ),
            encoding="utf-8",
        )
        (self.source / "skills" / "treework" / "SKILL.md").write_text(
            "# TreeWork\n",
            encoding="utf-8",
        )
        wrapper = self.source / "skills" / "treework" / "scripts" / "tw"
        wrapper.write_text("#!/usr/bin/env bash\n", encoding="utf-8")
        wrapper.chmod(0o755)
        (self.source / "hooks" / "hooks.json").write_text(
            '{"hooks":{"PreToolUse":[{"command":"$PLUGIN_ROOT/check"}],'
            '"Stop":[{"command":"$PLUGIN_ROOT/check"}]}}\n',
            encoding="utf-8",
        )
        self.cache = (
            self.code_home
            / "plugins"
            / "cache"
            / check_activation.MARKETPLACE_NAME
            / check_activation.PLUGIN_NAME
            / self.version
        )
        shutil.copytree(self.source, self.cache)
        self.plugin_payload = {
            "installed": [
                {
                    "pluginId": check_activation.PLUGIN_SELECTOR,
                    "name": check_activation.PLUGIN_NAME,
                    "marketplaceName": check_activation.MARKETPLACE_NAME,
                    "version": self.version,
                    "installed": True,
                    "enabled": True,
                    "source": {
                        "source": "local",
                        "path": str(self.source),
                    },
                    "marketplaceSource": {
                        "sourceType": "local",
                        "source": str(self.repository),
                    },
                }
            ],
            "available": [],
        }
        relative_skill = (
            f"{check_activation.PLUGIN_NAME}/{self.version}/"
            "skills/treework/SKILL.md"
        )
        self.prompt_payload = [
            {
                "content": [
                    {
                        "text": (
                            "TreeWork\n"
                            "treework:treework "
                            f"(file: cache/{relative_skill})"
                        )
                    }
                ]
            }
        ]
        self.runner_calls: list[list[str]] = []

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def runner(self, args: list[str]) -> str:
        self.runner_calls.append(args)
        if args[:3] == ["codex", "plugin", "list"]:
            return json.dumps(self.plugin_payload)
        if args[:3] == ["codex", "debug", "prompt-input"]:
            return json.dumps(self.prompt_payload)
        expected_wrapper = (
            self.cache
            / "skills"
            / "treework"
            / "scripts"
            / "tw"
        )
        if (
            len(args) == 4
            and args[0] == "env"
            and args[1].startswith("TREEWORK_BUILD_DIR=")
            and args[2:] == [str(expected_wrapper), "version"]
        ):
            return f"tw {self.version}\n"
        raise AssertionError(f"unexpected command: {args}")

    def attest(self) -> check_activation.Candidate:
        return check_activation.attest_activation(
            self.repository,
            self.code_home,
            self.runner,
        )

    def test_exact_candidate_passes(self) -> None:
        candidate = self.attest()
        self.assertEqual(candidate.version, self.version)
        self.assertEqual(candidate.cache_root, self.cache)
        self.assertIn(
            [
                "codex",
                "debug",
                "prompt-input",
                "-c",
                "mcp_servers={}",
                "noop",
            ],
            self.runner_calls,
        )

    def test_stale_installed_version_is_rejected(self) -> None:
        self.plugin_payload["installed"][0]["version"] = "0.1.0+stale"
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "does not match plugin source version",
        ):
            self.attest()

    def test_wrong_marketplace_root_is_rejected(self) -> None:
        wrong = Path(self.temporary.name) / "wrong-marketplace"
        wrong.mkdir()
        self.plugin_payload["installed"][0]["marketplaceSource"]["source"] = str(wrong)
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "marketplaceSource does not match",
        ):
            self.attest()

    def test_wrong_plugin_source_is_rejected(self) -> None:
        wrong = Path(self.temporary.name) / "wrong-plugin"
        wrong.mkdir()
        self.plugin_payload["installed"][0]["source"]["path"] = str(wrong)
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "source.path does not match",
        ):
            self.attest()

    def test_stale_prompt_version_is_rejected(self) -> None:
        prompt = self.prompt_payload[0]["content"][0]["text"]
        self.prompt_payload[0]["content"][0]["text"] = prompt.replace(
            self.version, "0.1.0+stale"
        )
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "exact candidate cache version",
        ):
            self.attest()

    def test_cache_byte_mismatch_is_rejected(self) -> None:
        (self.cache / "skills" / "treework" / "SKILL.md").write_text(
            "# stale cached skill\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "cache bytes differ from source",
        ):
            self.attest()

    def test_stale_cli_build_version_is_rejected(self) -> None:
        original_runner = self.runner

        def stale_runner(args: list[str]) -> str:
            if args[-1:] == ["version"] and Path(args[-2]).name == "tw":
                return "tw 0.1.0+stale\n"
            return original_runner(args)

        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "expected exact build",
        ):
            check_activation.attest_activation(
                self.repository,
                self.code_home,
                stale_runner,
            )

    def test_command_failure_is_converted_to_activation_error(self) -> None:
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "failed: fixture command failure",
        ):
            check_activation.run(
                [
                    sys.executable,
                    "-c",
                    "import sys; print('fixture command failure', file=sys.stderr); sys.exit(7)",
                ],
                cwd=self.repository,
                timeout_seconds=1,
            )

    def assert_process_gone(self, pid: int) -> None:
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            time.sleep(0.02)
        self.fail(f"timed-out descendant process {pid} is still alive")

    @unittest.skipUnless(os.name == "posix", "process-group assertion requires POSIX")
    def test_timeout_cleans_descendant_process_group(self) -> None:
        descendant_pid = Path(self.temporary.name) / "descendant.pid"
        program = (
            "import pathlib, subprocess, sys, time; "
            "child = subprocess.Popen([sys.executable, '-c', "
            "'import time; time.sleep(60)']); "
            f"pathlib.Path({str(descendant_pid)!r}).write_text(str(child.pid)); "
            "time.sleep(60)"
        )
        started = time.monotonic()
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "timed out after 0.25 seconds; terminated process group",
        ):
            check_activation.run(
                [sys.executable, "-c", program],
                cwd=self.repository,
                timeout_seconds=0.25,
                termination_grace_seconds=0.1,
            )
        self.assertLess(time.monotonic() - started, 2)
        self.assertTrue(descendant_pid.is_file())
        pid = int(descendant_pid.read_text(encoding="utf-8"))
        self.assert_process_gone(pid)

    @unittest.skipUnless(os.name == "posix", "process-group assertion requires POSIX")
    def test_timeout_cleans_descendant_after_leader_exits(self) -> None:
        descendant_pid = Path(self.temporary.name) / "early-exit-descendant.pid"
        program = (
            "import pathlib, subprocess, sys; "
            "child = subprocess.Popen([sys.executable, '-c', "
            "'import time; time.sleep(60)']); "
            f"pathlib.Path({str(descendant_pid)!r}).write_text(str(child.pid))"
        )
        with self.assertRaisesRegex(
            check_activation.ActivationError,
            "timed out after 0.25 seconds; terminated process group",
        ):
            check_activation.run(
                [sys.executable, "-c", program],
                cwd=self.repository,
                timeout_seconds=0.25,
                termination_grace_seconds=0.1,
            )
        self.assertTrue(descendant_pid.is_file())
        self.assert_process_gone(
            int(descendant_pid.read_text(encoding="utf-8"))
        )


if __name__ == "__main__":
    unittest.main()
