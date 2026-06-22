.PHONY: help dev build check test fmt lint clean install

# Default target: show help
help:
	@echo "Codex Timeline development commands"
	@echo ""
	@echo "  make dev      - Run the Tauri desktop app in development mode"
	@echo "  make build    - Build the Tauri desktop app"
	@echo "  make check    - Type check frontend and Rust backend"
	@echo "  make test     - Run Rust tests"
	@echo "  make fmt      - Format frontend and Rust code"
	@echo "  make lint     - Run Rust clippy and frontend type check"
	@echo "  make clean    - Remove build artifacts"
	@echo "  make install  - Install frontend dependencies and fetch Rust crates"

# Development mode (frontend + backend)
dev:
	pnpm tauri dev

# Build application (frontend + backend)
build:
	pnpm tauri build

# Type check (frontend + backend)
check:
	pnpm check
	cd src-tauri && cargo check

# Run tests
test:
	cd src-tauri && cargo test

# Format code (frontend + backend)
fmt:
	cd src-tauri && cargo fmt
	pnpm exec prettier --write "src/**/*.{ts,js,svelte}" 2>/dev/null || true

# Lint code (frontend + backend)
lint:
	cd src-tauri && cargo clippy
	pnpm check

# Clean build artifacts
clean:
	rm -rf build
	rm -rf .svelte-kit
	rm -rf src-tauri/target
	rm -rf node_modules/.vite

# Install dependencies
install:
	pnpm install
	cd src-tauri && cargo fetch
