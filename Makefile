BINARY ?= target/release/onekvm-protocol-rustdesk
PREFIX ?= /usr/local
DESTDIR ?=
VERSION ?= 0.0.0-dev
ARCH ?= unknown

.PHONY: all install

all:

install:
	test -x $(BINARY)
	install -Dm0755 $(BINARY) $(DESTDIR)$(PREFIX)/bin/onekvm-protocol-rustdesk
	install -Dm0644 web/page.json $(DESTDIR)$(PREFIX)/web/page.json
	install -Dm0644 web/rustdesk.svg $(DESTDIR)$(PREFIX)/web/rustdesk.svg
	install -d $(DESTDIR)$(PREFIX)
	sed -e 's/@ARCH@/$(ARCH)/g' -e 's/@VERSION@/$(VERSION)/g' \
		manifest.json.in >$(DESTDIR)$(PREFIX)/manifest.json
	chmod 0644 $(DESTDIR)$(PREFIX)/manifest.json
