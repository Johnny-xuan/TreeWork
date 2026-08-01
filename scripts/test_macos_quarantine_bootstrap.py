#!/usr/bin/env python3
"""Regression tests for the macOS quarantine-safe Rust bootstrap."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

from _paths import PLUGIN_ROOT


def write_executable(path: Path, source: str) -> None:
    path.write_text(textwrap.dedent(source).lstrip(), encoding="utf-8")
    path.chmod(0o755)


class MacOSQuarantineBootstrapTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(
            prefix="treework-quarantine-bootstrap-"
        )
        root = Path(self.temporary.name)
        self.plugin = root / "plugin"
        self.fake_bin = root / "bin"
        self.build = root / "build"
        self.log = root / "calls.log"

        wrapper_dir = self.plugin / "skills" / "treework" / "scripts"
        helper_dir = self.plugin / "scripts"
        source_dir = self.plugin / "crates" / "treework-cli" / "src"
        wrapper_dir.mkdir(parents=True)
        helper_dir.mkdir(parents=True)
        source_dir.mkdir(parents=True)
        (self.plugin / ".codex-plugin").mkdir(parents=True)
        self.fake_bin.mkdir()

        shutil.copy2(
            PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw",
            wrapper_dir / "tw",
        )
        shutil.copy2(
            PLUGIN_ROOT / "scripts" / "rustc-unquarantine.sh",
            helper_dir / "rustc-unquarantine.sh",
        )
        (source_dir / "main.rs").write_text("fn main() {}\n", encoding="utf-8")
        (self.plugin / "crates" / "treework-cli" / "Cargo.toml").write_text(
            '[package]\nname = "treework-cli"\nversion = "0.0.0"\n',
            encoding="utf-8",
        )
        (self.plugin / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        (self.plugin / "Cargo.lock").write_text("", encoding="utf-8")
        (self.plugin / ".codex-plugin" / "plugin.json").write_text(
            '{"name":"treework","version":"fixture"}\n', encoding="utf-8"
        )

        write_executable(
            self.fake_bin / "uname",
            """
            #!/usr/bin/env bash
            printf '%s\\n' "${TREEWORK_TEST_UNAME:-Darwin}"
            """,
        )
        write_executable(
            self.fake_bin / "xattr",
            """
            #!/usr/bin/env bash
            printf 'xattr:%s\\n' "$*" >> "$TREEWORK_TEST_LOG"
            """,
        )
        write_executable(
            self.fake_bin / "rustc",
            """
            #!/usr/bin/env bash
            printf 'rustc:%s\\n' "$*" >> "$TREEWORK_TEST_LOG"
            output=''
            while [[ "$#" -gt 0 ]]; do
              case "$1" in
                -o)
                  output="$2"
                  shift 2
                  ;;
                *) shift ;;
              esac
            done
            if [[ -z "$output" ]]; then
              printf 'rustc fixture\\n'
              exit 0
            fi
            mkdir -p "$(dirname "$output")"
            printf '#!/usr/bin/env bash\\nexit 0\\n' > "$output"
            chmod +x "$output"
            """,
        )
        write_executable(
            self.fake_bin / "inner-rustc-wrapper",
            """
            #!/usr/bin/env bash
            printf 'inner:%s\\n' "$*" >> "$TREEWORK_TEST_LOG"
            exec "$@"
            """,
        )
        write_executable(
            self.fake_bin / "cargo",
            """
            #!/usr/bin/env bash
            printf 'cargo-wrapper:%s\\n' "${RUSTC_WRAPPER:-}" >> "$TREEWORK_TEST_LOG"
            printf 'cargo-inner:%s\\n' "${TREEWORK_INNER_RUSTC_WRAPPER:-}" >> "$TREEWORK_TEST_LOG"
            out="$CARGO_TARGET_DIR/release/build/fixture"
            mkdir -p "$out" "$CARGO_TARGET_DIR/release"
            "$RUSTC_WRAPPER" "$TREEWORK_FAKE_RUSTC" -vV >/dev/null
            "$RUSTC_WRAPPER" "$TREEWORK_FAKE_RUSTC" \\
              --crate-name build_script_build \\
              --crate-type bin \\
              --out-dir "$out" \\
              -o "$out/build_script_build"
            printf '#!/usr/bin/env bash\\nprintf "tw fixture\\n"\\n' > "$CARGO_TARGET_DIR/release/tw"
            chmod +x "$CARGO_TARGET_DIR/release/tw"
            """,
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_wrapper(self, *, uname: str = "Darwin") -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.fake_bin}{os.pathsep}{env['PATH']}",
                "TREEWORK_BUILD_DIR": str(self.build),
                "TREEWORK_TEST_LOG": str(self.log),
                "TREEWORK_TEST_UNAME": uname,
                "TREEWORK_FAKE_RUSTC": str(self.fake_bin / "rustc"),
                "RUSTC_WRAPPER": str(self.fake_bin / "inner-rustc-wrapper"),
            }
        )
        return subprocess.run(
            [str(self.plugin / "skills" / "treework" / "scripts" / "tw"), "version"],
            cwd=self.plugin,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_darwin_chains_existing_wrapper_and_cleans_outputs(self) -> None:
        result = self.run_wrapper()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "tw fixture")
        calls = self.log.read_text(encoding="utf-8")
        helper = self.plugin / "scripts" / "rustc-unquarantine.sh"
        output_dir = self.build / "cargo-target" / "release" / "build" / "fixture"
        self.assertIn(f"cargo-wrapper:{helper}", calls)
        self.assertIn(
            f"cargo-inner:{self.fake_bin / 'inner-rustc-wrapper'}", calls
        )
        self.assertIn("inner:", calls)
        self.assertIn(f"xattr:-dr com.apple.quarantine {output_dir}", calls)
        self.assertIn(
            f"xattr:-d com.apple.quarantine {self.build / 'tw'}", calls
        )

    def test_non_darwin_keeps_existing_wrapper(self) -> None:
        result = self.run_wrapper(uname="Linux")
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text(encoding="utf-8")
        self.assertIn(
            f"cargo-wrapper:{self.fake_bin / 'inner-rustc-wrapper'}", calls
        )
        self.assertNotIn("xattr:", calls)


if __name__ == "__main__":
    unittest.main()
