# Installs Winkel from a source checkout or from a release tarball.
#
#   make                         build the release binary (needs cargo)
#   sudo make install            install to /usr/local
#   make install PREFIX=/usr DESTDIR=pkgdir   what a package does
#   sudo make uninstall          remove what install put down
#
# A release tarball carries a prebuilt binary beside this Makefile, so
# `make install` there needs no Rust toolchain.

NAME     := winkel
APP_ID   := dev.douglasmoura.winkel
PREFIX   ?= /usr/local
DESTDIR  ?=
BINDIR   := $(DESTDIR)$(PREFIX)/bin
SHAREDIR := $(DESTDIR)$(PREFIX)/share
BIN      ?= $(if $(wildcard ./$(NAME)),./$(NAME),target/release/$(NAME))

.PHONY: all build test install uninstall dist

all: build

build:
	cargo build --release --locked

test:
	cargo test --locked
	cargo test -p winkel-ui --features test-support --locked

install:
	@test -x "$(BIN)" || { echo "no binary at $(BIN): run 'make' first"; exit 1; }
	install -Dm755 "$(BIN)" "$(BINDIR)/$(NAME)"
	rm -rf "$(SHAREDIR)/$(NAME)/ui"
	install -d "$(SHAREDIR)/$(NAME)/ui"
	cp -r ui/i18n "$(SHAREDIR)/$(NAME)/ui/i18n"
	install -Dm644 packaging/$(APP_ID).desktop "$(SHAREDIR)/applications/$(APP_ID).desktop"
	install -Dm644 packaging/$(APP_ID).svg "$(SHAREDIR)/icons/hicolor/scalable/apps/$(APP_ID).svg"
	install -Dm644 packaging/$(APP_ID).metainfo.xml "$(SHAREDIR)/metainfo/$(APP_ID).metainfo.xml"
	install -Dm644 LICENSE "$(SHAREDIR)/licenses/$(NAME)/LICENSE"
	install -Dm644 packaging/ICON-LICENSE "$(SHAREDIR)/licenses/$(NAME)/ICON-LICENSE"

uninstall:
	rm -f "$(BINDIR)/$(NAME)"
	rm -rf "$(SHAREDIR)/$(NAME)"
	rm -f "$(SHAREDIR)/applications/$(APP_ID).desktop"
	rm -f "$(SHAREDIR)/icons/hicolor/scalable/apps/$(APP_ID).svg"
	rm -f "$(SHAREDIR)/metainfo/$(APP_ID).metainfo.xml"
	rm -rf "$(SHAREDIR)/licenses/$(NAME)"

# A release tarball for this machine's architecture: the binary, the
# catalogues, the packaging and this Makefile, so `make install` works
# without cargo.
VERSION  := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml 2>/dev/null | head -1)
ARCH     ?= $(shell uname -m)
DISTNAME := $(NAME)-$(VERSION)-$(ARCH)-linux

dist: build
	rm -rf "dist/$(DISTNAME)"
	install -d "dist/$(DISTNAME)/packaging" "dist/$(DISTNAME)/ui"
	install -m755 target/release/$(NAME) "dist/$(DISTNAME)/$(NAME)"
	cp -r ui/i18n "dist/$(DISTNAME)/ui/i18n"
	cp packaging/$(APP_ID).desktop packaging/$(APP_ID).svg packaging/$(APP_ID).metainfo.xml packaging/ICON-LICENSE "dist/$(DISTNAME)/packaging/"
	cp LICENSE README.md Makefile "dist/$(DISTNAME)/"
	tar -C dist -czf "dist/$(DISTNAME).tar.gz" "$(DISTNAME)"
	cd dist && sha256sum "$(DISTNAME).tar.gz" > "$(DISTNAME).tar.gz.sha256"
	@echo "dist/$(DISTNAME).tar.gz"
