# Long Live Flash — macOS Makefile
#
# Usage:
#   make build            Build desktop app and web extension
#   make build-desktop    Build desktop app (release)
#   make build-extension  Build web extension (and selfhosted bundle)
#   make run-desktop      Run desktop app in debug mode
#   make check-deps       Print prerequisite status (no build)
#   make install-deps     Install any missing prerequisites
#   make clean            Remove build artifacts

SHELL          := /bin/zsh
CARGO_BIN      := $(HOME)/.cargo/bin
CARGO          := $(CARGO_BIN)/cargo
RUSTUP         := $(CARGO_BIN)/rustup
WASM_BINDGEN   := $(CARGO_BIN)/wasm-bindgen
WASM_BINDGEN_VERSION := 0.2.120
PNPM           := pnpm
WEB_DIR        := web
DESKTOP_PKG    := llflash_desktop

# Make sure rustup's cargo (with wasm32-unknown-unknown) and wasm-bindgen
# are on PATH for every recipe — pnpm build scripts spawn cargo themselves.
export PATH    := $(CARGO_BIN):$(PATH)

.PHONY: help all build build-desktop build-extension build-rtmp-host \
        run-desktop install-rtmp-host \
        check-deps check-deps-desktop check-deps-extension \
        install-deps clean

help:
	@printf "Targets:\n"
	@printf "  build              Build desktop app and web extension\n"
	@printf "  build-desktop      Build desktop app (release)\n"
	@printf "  build-extension    Build web extension (and selfhosted bundle)\n"
	@printf "  build-rtmp-host    Build the MV3 native messaging host for RTMP (release)\n"
	@printf "  install-rtmp-host  Install the native host manifest [BROWSER=chrome] (macOS/Linux)\n"
	@printf "                     Windows: PowerShell -ExecutionPolicy Bypass -File native-host/install.ps1\n"
	@printf "  run-desktop        Run desktop app in debug mode\n"
	@printf "  check-deps         Print prerequisite status\n"
	@printf "  install-deps       Install any missing prerequisites\n"
	@printf "  clean              Remove build artifacts\n"

all: build

build: build-desktop build-extension

build-desktop: check-deps-desktop
	$(CARGO) build --release -p $(DESKTOP_PKG)

build-extension: check-deps-extension
	cd $(WEB_DIR) && $(PNPM) build

run-desktop: check-deps-desktop
	$(CARGO) run -p $(DESKTOP_PKG)

build-rtmp-host: check-deps-desktop
	$(CARGO) build --release -p llflash_rtmp_host

BROWSER ?= chrome
install-rtmp-host: build-rtmp-host
	bash native-host/install.sh "$(BROWSER)"

# ---------------------------------------------------------------------------
# Dependency checks
# ---------------------------------------------------------------------------

check-deps: check-deps-desktop check-deps-extension
	@printf "All prerequisites OK.\n"

check-deps-desktop:
	@missing=0; \
	if ! [ -x "$(CARGO)" ] && ! command -v cargo >/dev/null 2>&1; then \
		printf "MISSING  cargo (install rustup: 'make install-deps' or https://rustup.rs)\n"; \
		missing=1; \
	else printf "OK       cargo\n"; fi; \
	[ $$missing -eq 0 ] || { printf "\nRun 'make install-deps' to install missing tools.\n"; exit 1; }

check-deps-extension: check-deps-desktop
	@missing=0; \
	if ! command -v node >/dev/null 2>&1; then \
		printf "MISSING  node (install Node.js >= 24 from https://nodejs.org)\n"; missing=1; \
	else printf "OK       node ($$(node --version))\n"; fi; \
	if ! command -v $(PNPM) >/dev/null 2>&1; then \
		printf "MISSING  pnpm (install via 'corepack enable' or 'npm install -g pnpm')\n"; missing=1; \
	else printf "OK       pnpm ($$($(PNPM) --version))\n"; fi; \
	if ! command -v java >/dev/null 2>&1; then \
		printf "MISSING  java (install JDK 17+)\n"; missing=1; \
	else printf "OK       java\n"; fi; \
	if [ -x "$(RUSTUP)" ] && $(RUSTUP) target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$$'; then \
		printf "OK       wasm32-unknown-unknown target\n"; \
	else \
		printf "MISSING  wasm32-unknown-unknown target\n"; missing=1; \
	fi; \
	if [ -x "$(WASM_BINDGEN)" ]; then \
		printf "OK       wasm-bindgen ($$($(WASM_BINDGEN) --version | awk '{print $$2}'))\n"; \
	else \
		printf "MISSING  wasm-bindgen-cli v$(WASM_BINDGEN_VERSION)\n"; missing=1; \
	fi; \
	if [ -d "$(WEB_DIR)/node_modules" ]; then \
		printf "OK       web/node_modules\n"; \
	else \
		printf "MISSING  web/node_modules (pnpm install in $(WEB_DIR)/)\n"; missing=1; \
	fi; \
	[ $$missing -eq 0 ] || { printf "\nRun 'make install-deps' to install missing tools.\n"; exit 1; }

# ---------------------------------------------------------------------------
# Auto-install missing prerequisites (idempotent)
# ---------------------------------------------------------------------------

install-deps:
	@if ! [ -x "$(RUSTUP)" ]; then \
		printf "Installing rustup (non-interactive)...\n"; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
			sh -s -- -y --default-toolchain stable --profile minimal; \
	else printf "rustup already installed\n"; fi
	@if ! $(RUSTUP) target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$$'; then \
		printf "Adding wasm32-unknown-unknown target...\n"; \
		$(RUSTUP) target add wasm32-unknown-unknown; \
	else printf "wasm32-unknown-unknown target already installed\n"; fi
	@if ! [ -x "$(WASM_BINDGEN)" ]; then \
		printf "Installing wasm-bindgen-cli v$(WASM_BINDGEN_VERSION)...\n"; \
		$(CARGO) install wasm-bindgen-cli --version $(WASM_BINDGEN_VERSION); \
	else printf "wasm-bindgen-cli already installed\n"; fi
	@if ! command -v node >/dev/null 2>&1; then \
		printf "ERROR: Node.js is required — install it from https://nodejs.org or via Homebrew: brew install node\n"; \
		exit 1; \
	fi
	@if ! command -v java >/dev/null 2>&1; then \
		printf "ERROR: Java is required — install JDK 17+ (e.g. brew install openjdk@17)\n"; \
		exit 1; \
	fi
	@if ! command -v $(PNPM) >/dev/null 2>&1; then \
		if command -v corepack >/dev/null 2>&1; then \
			printf "Enabling pnpm via corepack...\n"; \
			corepack enable; \
		else \
			printf "ERROR: pnpm is required — install via 'corepack enable' (bundled with Node 16.13+) or 'npm install -g pnpm'\n"; \
			exit 1; \
		fi; \
	else printf "pnpm already installed\n"; fi
	@if ! [ -d "$(WEB_DIR)/node_modules" ]; then \
		printf "Running pnpm install in $(WEB_DIR)/...\n"; \
		cd $(WEB_DIR) && $(PNPM) install; \
	else printf "web/node_modules already present\n"; fi
	@printf "\nAll prerequisites installed.\n"

# ---------------------------------------------------------------------------

clean:
	$(CARGO) clean
	rm -rf $(WEB_DIR)/packages/core/dist
	rm -rf $(WEB_DIR)/packages/demo/dist
	rm -rf $(WEB_DIR)/packages/extension/dist
	rm -rf $(WEB_DIR)/packages/extension/assets/dist
	rm -rf $(WEB_DIR)/packages/selfhosted/dist
