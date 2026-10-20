PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
MANDIR  ?= $(PREFIX)/share/man/man1
BASHDIR ?= $(PREFIX)/share/bash-completion/completions
ZSHDIR  ?= $(PREFIX)/share/zsh/site-functions
FISHDIR ?= $(PREFIX)/share/fish/vendor_completions.d
BINS     = mdspeak mdhear

.PHONY: all build test install uninstall clean

all build:
	cargo build --release --locked

test:
	cargo test --locked

install: build
	install -d $(DESTDIR)$(BINDIR) $(DESTDIR)$(MANDIR) $(DESTDIR)$(BASHDIR) $(DESTDIR)$(ZSHDIR) $(DESTDIR)$(FISHDIR)
	for b in $(BINS); do \
	  install -m 755 target/release/$$b $(DESTDIR)$(BINDIR)/$$b; \
	  install -m 644 man/$$b.1 $(DESTDIR)$(MANDIR)/$$b.1; \
	  target/release/$$b --completions bash > $(DESTDIR)$(BASHDIR)/$$b; \
	  target/release/$$b --completions zsh  > $(DESTDIR)$(ZSHDIR)/_$$b; \
	  target/release/$$b --completions fish > $(DESTDIR)$(FISHDIR)/$$b.fish; \
	done

uninstall:
	for b in $(BINS); do \
	  rm -f $(DESTDIR)$(BINDIR)/$$b $(DESTDIR)$(MANDIR)/$$b.1 \
	        $(DESTDIR)$(BASHDIR)/$$b $(DESTDIR)$(ZSHDIR)/_$$b $(DESTDIR)$(FISHDIR)/$$b.fish; \
	done

clean:
	cargo clean
