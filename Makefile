PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
CARGO ?= cargo
INSTALL ?= install
SKILLDIR ?= $(HOME)/.agents/skills

.PHONY: build check install install-skill uninstall

build:
	$(CARGO) build --release

check:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets --all-features -- -D warnings
	$(CARGO) test --all-targets --all-features

install: build
	$(INSTALL) -d "$(DESTDIR)$(BINDIR)"
	$(INSTALL) -m 755 target/release/dev "$(DESTDIR)$(BINDIR)/dev"

install-skill:
	$(INSTALL) -d "$(DESTDIR)$(SKILLDIR)/dev-connect"
	$(INSTALL) -m 644 skills/dev-connect/SKILL.md "$(DESTDIR)$(SKILLDIR)/dev-connect/SKILL.md"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/dev"

