.PHONY: help dev dev-mock-update build check test fmt lint clean install

DEV_PORT ?= 1420
DEV_PORT_ARG := $(word 2,$(MAKECMDGOALS))
DEV_TARGETS := dev dev-mock-update

# Allow the port as a positional argument: make dev 1420
ifneq ($(filter $(DEV_TARGETS),$(MAKECMDGOALS)),)
ifneq ($(DEV_PORT_ARG),)
DEV_PORT := $(DEV_PORT_ARG)
.PHONY: $(DEV_PORT_ARG)
$(DEV_PORT_ARG):
	@:
endif
endif

# Default target: show help
help:
	@echo "HarnessLens development commands"
	@echo ""
	@echo "  make dev [port] - Run the Tauri desktop app (default port: 1420)"
	@echo "  make dev-mock-update [port] - Run the Tauri desktop app with mock update enabled"
	@echo "  make build    - Build the Tauri desktop app"
	@echo "  make check    - Type check frontend and Rust backend"
	@echo "  make test     - Run end-to-end and Rust tests"
	@echo "  make fmt      - Format frontend and Rust code"
	@echo "  make lint     - Run Rust clippy and frontend type check"
	@echo "  make clean    - Remove build artifacts"
	@echo "  make install  - Install frontend dependencies and fetch Rust crates"

# Development mode (frontend + backend)
dev:
	@case "$(DEV_PORT)" in \
		''|*[!0-9]*) echo "Usage: make dev [port] (port must be a number from 1 to 65535)"; exit 2;; \
		*) if [ "$(DEV_PORT)" -lt 1 ] || [ "$(DEV_PORT)" -gt 65535 ]; then echo "Port must be between 1 and 65535"; exit 2; fi;; \
	esac
	VITE_PORT=$(DEV_PORT) pnpm tauri dev \
		--config src-tauri/tauri.dev.conf.json \
		--config '{"build":{"devUrl":"http://localhost:$(DEV_PORT)"}}'

dev-mock-update:
	@case "$(DEV_PORT)" in \
		''|*[!0-9]*) echo "Usage: make dev-mock-update [port] (port must be a number from 1 to 65535)"; exit 2;; \
		*) if [ "$(DEV_PORT)" -lt 1 ] || [ "$(DEV_PORT)" -gt 65535 ]; then echo "Port must be between 1 and 65535"; exit 2; fi;; \
	esac
	VITE_PORT=$(DEV_PORT) VITE_MOCK_APP_UPDATE=1 pnpm tauri dev \
		--config src-tauri/tauri.dev.conf.json \
		--config '{"build":{"devUrl":"http://localhost:$(DEV_PORT)"}}'

# Build application (frontend + backend)
build:
	pnpm tauri build

# Type check (frontend + backend)
check:
	pnpm check
	cargo check --workspace --all-targets

# Run tests
test:
	pnpm test
	cargo test --workspace

# Format code (frontend + backend)
fmt:
	cargo fmt --all

# Lint code (frontend + backend)
lint:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	pnpm check

# Clean build artifacts
clean:
	rm -rf build
	rm -rf .svelte-kit
	cargo clean
	rm -rf node_modules/.vite

# Install dependencies
install:
	pnpm install
	cargo fetch
