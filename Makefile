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
	$(INSTALL) -d "$(DESTDIR)$(SKILLDIR)/dev-cli"
	$(INSTALL) -m 644 skills/dev-cli/SKILL.md "$(DESTDIR)$(SKILLDIR)/dev-cli/SKILL.md"
	@if grep -q '^name: dev-connect$$' "$(DESTDIR)$(SKILLDIR)/dev-connect/SKILL.md" 2>/dev/null; then \
		$(RM) "$(DESTDIR)$(SKILLDIR)/dev-connect/SKILL.md"; \
		rmdir "$(DESTDIR)$(SKILLDIR)/dev-connect" 2>/dev/null || true; \
	fi

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/dev"
