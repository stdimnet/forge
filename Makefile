.PHONY: dev install

dev:
	cd app/ui && pnpm tauri dev

install:
	cd app/ui && pnpm install
