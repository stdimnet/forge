.PHONY: dev install

# On macOS the debug binary runs from a small .app bundle (scripts/macos-dev-runner.sh), so
# forge:// links (Code With Me invitations) open the running dev instance.
ifeq ($(shell uname -s),Darwin)
HOST := $(shell rustc -vV | sed -n 's/^host: //p')
RUNNER_VAR := CARGO_TARGET_$(shell echo $(HOST) | tr 'a-z-' 'A-Z_')_RUNNER
export $(RUNNER_VAR) := $(CURDIR)/scripts/macos-dev-runner.sh
endif

dev:
	cd app/ui && pnpm tauri dev

install:
	cd app/ui && pnpm install
