prefix ?= $(PREFIX)

# this avoids /usr/local/usr/* when
# installing with prefix=/usr/local
ifeq ($(prefix), /usr/local)
exec_prefix ?= $(prefix)
datarootdir ?= $(prefix)/share
else
exec_prefix ?= $(prefix)/usr
datarootdir ?= $(prefix)/usr/share
endif

bindir ?= $(exec_prefix)/bin
libdir ?= $(exec_prefix)/lib
datadir ?= $(datarootdir)
mandir ?= $(datarootdir)/man

CARGO ?= cargo
CARGO_FLAGS ?= --release --locked

BIN_T := $(DESTDIR)$(bindir)/elastic-swap
SVC_T := $(DESTDIR)$(libdir)/systemd/system/elastic-swap.service
DFL_T := $(DESTDIR)$(datadir)/elastic-swap/swap-default.conf
MAN5_T := $(DESTDIR)$(mandir)/man5/elastic-swap.conf.5
MAN8_T := $(DESTDIR)$(mandir)/man8/elastic-swap.8
# Vendor directory: /etc/sysctl.d stays free for an administrator to shadow it.
SYSCTL_T := $(DESTDIR)$(libdir)/sysctl.d/60-elastic-swap.conf

.PHONY: build files install uninstall clean help

default: build

build: ## Build Rust binary
	$(CARGO) build $(CARGO_FLAGS)

$(BIN_T): target/release/elastic-swap
	install -p -Dm755 $< $@

$(SVC_T): include/elastic-swap.service
	install -p -Dm644 $< $@

$(DFL_T): include/swap-default.conf
	install -p -Dm644 $< $@

$(MAN5_T): man/elastic-swap.conf.5
	install -p -Dm644 $< $@

$(MAN8_T): man/elastic-swap.8
	install -p -Dm644 $< $@

$(SYSCTL_T): include/elastic-swap-sysctl.conf
	install -p -Dm644 $< $@

target/release/elastic-swap: build

files: $(BIN_T) $(SVC_T) $(DFL_T) $(MAN5_T) $(MAN8_T) $(SYSCTL_T)

install: ## Install elastic-swap
install: build files

uninstall: ## Delete elastic-swap (stop elastic-swap first)
uninstall:
	! systemctl is-active --quiet elastic-swap.service
	rm -v $(BIN_T) $(SVC_T) $(DFL_T) $(MAN5_T) $(MAN8_T) $(SYSCTL_T)
	rmdir -v $(DESTDIR)$(datadir)/elastic-swap

clean: ## Remove build output
	$(CARGO) clean

help: ## Show help
	@grep -h "##" $(MAKEFILE_LIST) | grep -v grep | sed 's/\\$$//;s/##/\t/'
