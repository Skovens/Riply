PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin

.PHONY: install build frontend codespell

# Default: rebuild everything and make the fresh binary live at $(BINDIR)/riply
# via a symlink to target/release/riply.
install: frontend build
	install -d $(BINDIR)
	ln -sfn $(CURDIR)/target/release/riply $(BINDIR)/riply

build:
	cargo build --release

frontend:
	cd web/svelte && npm run build --silent

codespell:
	uv run codespell -c setup.cfg
