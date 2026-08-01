PLUGIN := plugins/treework
CLI_MANIFEST := $(PLUGIN)/crates/treework-cli/Cargo.toml
PLUGIN_CREATOR ?= $(HOME)/.codex/skills/.system/plugin-creator
SKILL_CREATOR ?= $(HOME)/.codex/skills/.system/skill-creator
ARTIFACTS ?= .artifacts

.PHONY: test test-rust test-ui test-runtime validate build-ui package browser-test

test: test-rust test-ui test-runtime

test-rust:
	cargo test --manifest-path $(CLI_MANIFEST)

test-ui:
	cd project-map-ui && npm test
	cd project-map-ui && npm run typecheck

test-runtime:
	python3 scripts/check_cli_regression.py
	python3 scripts/check_project_map_read_model.py
	python3 scripts/check_hooks.py
	python3 scripts/check_mcp.py
	python3 scripts/test_check_activation.py

build-ui:
	cd project-map-ui && npm run build

validate: build-ui
	git diff --exit-code -- $(PLUGIN)/assets/graph-panel
	python3 scripts/check_packaging.py
	python3 $(PLUGIN_CREATOR)/scripts/validate_plugin.py $(PLUGIN)
	python3 $(SKILL_CREATOR)/scripts/quick_validate.py \
		$(PLUGIN)/skills/treework

package:
	python3 scripts/package_plugin.py
	python3 scripts/check_package_commit_source.py

browser-test:
	mkdir -p $(ARTIFACTS)
	python3 scripts/check_project_map_browser.py
	python3 scripts/check_project_map_installed.py
	python3 scripts/measure_project_map_performance.py --mode verify \
		--output $(ARTIFACTS)/project-map-performance.json
