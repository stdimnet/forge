.PHONY: dev install

# Hosting a Code With Me session needs the dev flag until host approval exists.
dev:
	cd app/ui && FORGE_CWM_DEV=1 pnpm tauri dev

install:
	cd app/ui && pnpm install
