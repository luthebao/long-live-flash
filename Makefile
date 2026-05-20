# Long Live Flash — macOS Makefile
#
# Usage:
#   make build           Build desktop app and web extension
#   make build-desktop   Build desktop app (release)
#   make build-extension Build web extension (and selfhosted bundle)
#   make run-desktop     Run desktop app in debug mode
#   make clean           Remove build artifacts

SHELL       := /bin/zsh
CARGO_BIN   := $(HOME)/.cargo/bin
CARGO       := $(CARGO_BIN)/cargo
NPM         := npm
WEB_DIR     := web
DESKTOP_PKG := llflash_desktop

# Make sure rustup's cargo (with wasm32-unknown-unknown) and wasm-bindgen
# are on PATH for every recipe — npm build scripts spawn cargo themselves.
export PATH := $(CARGO_BIN):$(PATH)

.PHONY: help all build build-desktop build-extension run-desktop clean

help:
	@printf "Targets:\n"
	@printf "  build            Build desktop app and web extension\n"
	@printf "  build-desktop    Build desktop app (release)\n"
	@printf "  build-extension  Build web extension (and selfhosted bundle)\n"
	@printf "  run-desktop      Run desktop app in debug mode\n"
	@printf "  clean            Remove build artifacts\n"

all: build

build: build-desktop build-extension

build-desktop:
	$(CARGO) build --release -p $(DESKTOP_PKG)

build-extension:
	cd $(WEB_DIR) && $(NPM) install
	cd $(WEB_DIR) && $(NPM) run build

run-desktop:
	$(CARGO) run -p $(DESKTOP_PKG)

clean:
	$(CARGO) clean
	rm -rf $(WEB_DIR)/packages/core/dist
	rm -rf $(WEB_DIR)/packages/demo/dist
	rm -rf $(WEB_DIR)/packages/extension/dist
	rm -rf $(WEB_DIR)/packages/extension/assets/dist
	rm -rf $(WEB_DIR)/packages/selfhosted/dist
