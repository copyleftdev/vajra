# ============================================================================
#  VAJRA — Break noise. Preserve truth.
#  Master Makefile for development, testing, release, and documentation.
# ============================================================================

SHELL       := /bin/bash
.DEFAULT_GOAL := help

# Version from Cargo workspace
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
GIT_SHA     := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BINARY      := vajra
CORPUS_DIR  := corpus
GOLDEN_DIR  := corpus/golden
DOCS_DIR    := docs
MAN_DIR     := man

# Install paths (override with: make install-local PREFIX=/opt/vajra)
PREFIX      := /usr/local
BINDIR      := $(PREFIX)/bin
MANDIR      := $(PREFIX)/share/man/man1

# Rust tooling
CARGO       := cargo
CLIPPY      := cargo clippy
FMT         := cargo fmt
BENCH       := cargo bench
MDBOOK      := mdbook

# Build profiles
RELEASE_FLAGS := --release --bin $(BINARY)

# Test configuration
PROPTEST_NORMAL  := 1000
PROPTEST_EXTENDED := 100000

# Colors for terminal output
C_GOLD    := \033[33m
C_GREEN   := \033[32m
C_RED     := \033[31m
C_CYAN    := \033[36m
C_DIM     := \033[2m
C_BOLD    := \033[1m
C_RESET   := \033[0m

# ============================================================================
#  HELP
# ============================================================================

.PHONY: help
help: ## Show this help
	@echo ""
	@echo -e "  $(C_GOLD)$(C_BOLD)VAJRA$(C_RESET) $(C_DIM)v$(VERSION)$(C_RESET)"
	@echo -e "  $(C_DIM)Break noise. Preserve truth.$(C_RESET)"
	@echo ""
	@echo -e "  $(C_BOLD)Usage:$(C_RESET) make $(C_CYAN)<target>$(C_RESET)"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  $(C_CYAN)%-20s$(C_RESET) %s\n", $$1, $$2}'
	@echo ""

# ============================================================================
#  BUILD
# ============================================================================

.PHONY: build
build: ## Build all crates (debug)
	@echo -e "$(C_GOLD)Building workspace...$(C_RESET)"
	@$(CARGO) build --workspace

.PHONY: release
release: ## Build optimized release binary
	@echo -e "$(C_GOLD)Building release binary...$(C_RESET)"
	@$(CARGO) build $(RELEASE_FLAGS)
	@echo -e "$(C_GREEN)Binary: target/release/$(BINARY)$(C_RESET)"
	@ls -lh target/release/$(BINARY)

.PHONY: install
install: ## Install vajra to ~/.cargo/bin via cargo
	@echo -e "$(C_GOLD)Installing vajra via cargo...$(C_RESET)"
	@$(CARGO) install --path vajra-cli --bin $(BINARY)
	@echo -e "$(C_GREEN)Installed: $$(which vajra)$(C_RESET)"

.PHONY: install-local
install-local: ## Install binary + man page to PREFIX (default /usr/local)
	@if [ ! -f target/release/$(BINARY) ]; then \
		echo -e "$(C_RED)Release binary not found. Run 'make release' first.$(C_RESET)"; \
		exit 1; \
	fi
	@echo -e "$(C_GOLD)Installing to $(PREFIX)...$(C_RESET)"
	@install -d $(BINDIR)
	@install -m 755 target/release/$(BINARY) $(BINDIR)/$(BINARY)
	@echo -e "$(C_GREEN)  $(BINDIR)/$(BINARY)$(C_RESET)"
	@install -d $(MANDIR)
	@install -m 644 $(MAN_DIR)/$(BINARY).1 $(MANDIR)/$(BINARY).1
	@echo -e "$(C_GREEN)  $(MANDIR)/$(BINARY).1$(C_RESET)"
	@echo -e "$(C_GREEN)Done. Run 'vajra --help' or 'man vajra'.$(C_RESET)"

.PHONY: uninstall-local
uninstall-local: ## Remove binary + man page from PREFIX
	@echo -e "$(C_GOLD)Removing from $(PREFIX)...$(C_RESET)"
	@rm -f $(BINDIR)/$(BINARY)
	@rm -f $(MANDIR)/$(BINARY).1
	@echo -e "$(C_DIM)Removed.$(C_RESET)"

.PHONY: clean
clean: ## Remove build artifacts
	@$(CARGO) clean
	@rm -rf $(DOCS_DIR)/book
	@echo -e "$(C_DIM)Cleaned.$(C_RESET)"

# ============================================================================
#  MAN PAGES
# ============================================================================

.PHONY: man-build
man-build: ## Build man pages (verify with man-preview)
	@echo -e "$(C_GOLD)Man pages ready: $(MAN_DIR)/$(BINARY).1$(C_RESET)"

.PHONY: man-preview
man-preview: ## Preview the man page in terminal
	@man $(MAN_DIR)/$(BINARY).1

.PHONY: man-lint
man-lint: ## Check man page for errors
	@echo -e "$(C_GOLD)Linting man pages...$(C_RESET)"
	@mandoc -T lint $(MAN_DIR)/$(BINARY).1 2>&1 || true
	@echo -e "$(C_GREEN)Done.$(C_RESET)"

# ============================================================================
#  TEST
# ============================================================================

.PHONY: test
test: ## Run all tests
	@echo -e "$(C_GOLD)Running test suite...$(C_RESET)"
	@$(CARGO) test --workspace
	@echo -e "$(C_GREEN)All tests passed.$(C_RESET)"

.PHONY: test-quick
test-quick: ## Run tests without property/golden tests (fast)
	@echo -e "$(C_GOLD)Quick test run...$(C_RESET)"
	@$(CARGO) test --workspace -- --skip prop_ --skip golden --skip chaos --skip determinism --skip differential

.PHONY: test-property
test-property: ## Run property tests (1K cases per test)
	@echo -e "$(C_GOLD)Property tests ($(PROPTEST_NORMAL) cases)...$(C_RESET)"
	@$(CARGO) test --workspace -- prop_

.PHONY: test-property-extended
test-property-extended: ## Run property tests (100K cases — CI level)
	@echo -e "$(C_GOLD)Extended property tests ($(PROPTEST_EXTENDED) cases)...$(C_RESET)"
	@PROPTEST_CASES=$(PROPTEST_EXTENDED) $(CARGO) test --workspace -- prop_

.PHONY: test-chaos
test-chaos: ## Run chaos/adversarial input tests
	@echo -e "$(C_GOLD)Chaos tests...$(C_RESET)"
	@$(CARGO) test --workspace -- chaos

.PHONY: test-determinism
test-determinism: ## Run determinism verification (10 runs per stage)
	@echo -e "$(C_GOLD)Determinism verification...$(C_RESET)"
	@$(CARGO) test --workspace -- determinism

.PHONY: test-differential
test-differential: ## Run differential tests (exact vs streaming)
	@echo -e "$(C_GOLD)Differential tests...$(C_RESET)"
	@$(CARGO) test --workspace -- differential

.PHONY: test-golden
test-golden: ## Run golden regression tests against corpus
	@echo -e "$(C_GOLD)Golden tests...$(C_RESET)"
	@$(CARGO) test -p vajra-core -- golden

.PHONY: test-all
test-all: test test-property-extended ## Run everything including extended property tests
	@echo -e "$(C_GREEN)Complete test suite passed.$(C_RESET)"

# ============================================================================
#  QUALITY
# ============================================================================

.PHONY: fmt
fmt: ## Format all code
	@$(FMT) --all
	@echo -e "$(C_DIM)Formatted.$(C_RESET)"

.PHONY: fmt-check
fmt-check: ## Check formatting without modifying
	@$(FMT) --all --check

.PHONY: clippy
clippy: ## Run clippy lints
	@$(CLIPPY) --workspace

.PHONY: lint
lint: fmt-check clippy ## Run all lints (format + clippy)
	@echo -e "$(C_GREEN)All lints passed.$(C_RESET)"

.PHONY: check
check: ## Quick compile check (no codegen)
	@$(CARGO) check --workspace

.PHONY: gate
gate: lint test test-golden ## Full quality gate (lint + test + golden)
	@echo -e ""
	@echo -e "$(C_GREEN)$(C_BOLD)Quality gate passed.$(C_RESET)"

# ============================================================================
#  BENCHMARKS
# ============================================================================

.PHONY: bench
bench: ## Run all benchmarks
	@echo -e "$(C_GOLD)Running benchmarks...$(C_RESET)"
	@$(BENCH) --workspace

.PHONY: bench-parse
bench-parse: ## Benchmark JSON parsing
	@$(BENCH) -p vajra-core --bench parse

.PHONY: bench-stats
bench-stats: ## Benchmark statistical analysis
	@$(BENCH) -p vajra-stats --bench stats

.PHONY: bench-sketches
bench-sketches: ## Benchmark streaming data structures
	@$(BENCH) -p vajra-stats --bench sketches

.PHONY: bench-fingerprint
bench-fingerprint: ## Benchmark fingerprinting
	@$(BENCH) -p vajra-fingerprint --bench fingerprint

.PHONY: bench-essence
bench-essence: ## Benchmark essence generation
	@$(BENCH) -p vajra-essence --bench essence

# ============================================================================
#  CORPUS & GOLDEN
# ============================================================================

.PHONY: corpus
corpus: ## Generate test corpus and golden files
	@echo -e "$(C_GOLD)Generating corpus...$(C_RESET)"
	@$(CARGO) run --bin generate-corpus
	@echo -e "$(C_GREEN)Corpus: $(CORPUS_DIR)/ ($(shell find $(CORPUS_DIR) -type f -not -path '*/golden/*' | wc -l) files)$(C_RESET)"
	@echo -e "$(C_GREEN)Golden: $(GOLDEN_DIR)/ ($(shell find $(GOLDEN_DIR) -type f | wc -l) captures)$(C_RESET)"

.PHONY: corpus-verify
corpus-verify: corpus test-golden ## Regenerate corpus and verify golden tests pass
	@echo -e "$(C_GREEN)Corpus verified.$(C_RESET)"

# ============================================================================
#  DOCUMENTATION
# ============================================================================

.PHONY: docs
docs: ## Build mdbook documentation
	@echo -e "$(C_GOLD)Building docs...$(C_RESET)"
	@cd $(DOCS_DIR) && $(MDBOOK) build
	@echo -e "$(C_GREEN)Docs: $(DOCS_DIR)/book/index.html$(C_RESET)"

.PHONY: docs-serve
docs-serve: ## Serve docs locally with live reload
	@echo -e "$(C_GOLD)Serving docs at http://localhost:3000 ...$(C_RESET)"
	@cd $(DOCS_DIR) && $(MDBOOK) serve --open

.PHONY: docs-clean
docs-clean: ## Remove built docs
	@rm -rf $(DOCS_DIR)/book
	@echo -e "$(C_DIM)Docs cleaned.$(C_RESET)"

# ============================================================================
#  RELEASE
# ============================================================================

.PHONY: release-tag
release-tag: gate ## Tag a release (usage: make release-tag V=0.2.0)
ifndef V
	$(error Usage: make release-tag V=0.2.0)
endif
	@echo -e "$(C_GOLD)Tagging v$(V)...$(C_RESET)"
	git tag -a v$(V) -m "Vajra v$(V)"
	git push origin v$(V)
	@echo -e "$(C_GREEN)Tag v$(V) pushed. Release workflow will build binaries.$(C_RESET)"

.PHONY: release-local
release-local: ## Build release binaries for current platform
	@echo -e "$(C_GOLD)Building local release...$(C_RESET)"
	@$(CARGO) build $(RELEASE_FLAGS)
	@mkdir -p dist
	@cp target/release/$(BINARY) dist/$(BINARY)
	@cd dist && tar czf $(BINARY)-v$(VERSION)-$$(rustc -vV | grep host | awk '{print $$2}').tar.gz $(BINARY)
	@echo -e "$(C_GREEN)Package: dist/$(C_RESET)"
	@ls -lh dist/*.tar.gz

# ============================================================================
#  DEVELOPMENT WORKFLOWS
# ============================================================================

.PHONY: dev
dev: fmt check test-quick ## Fast dev cycle: format + check + quick tests
	@echo -e "$(C_GREEN)Dev cycle clean.$(C_RESET)"

.PHONY: pre-commit
pre-commit: fmt lint test test-golden ## Pre-commit checks: format + lint + test + golden
	@echo -e "$(C_GREEN)Ready to commit.$(C_RESET)"

.PHONY: ci-local
ci-local: lint test test-property-extended test-golden ## Simulate full CI locally
	@echo -e "$(C_GREEN)CI simulation passed.$(C_RESET)"

.PHONY: demo
demo: build ## Run a demo on sample data
	@echo -e "$(C_GOLD)--- inspect ---$(C_RESET)"
	@echo '{"patient":{"name":"John","dob":"1985-03-15"},"codes":["E11.9","I10"],"amount":150}' | \
		$(CARGO) run --bin $(BINARY) -- inspect -
	@echo ""
	@echo -e "$(C_GOLD)--- anomalies ---$(C_RESET)"
	@$(CARGO) run --bin $(BINARY) -- anomalies corpus/medical/single_claim.json 2>/dev/null || true
	@echo ""
	@echo -e "$(C_GOLD)--- essence (staff) ---$(C_RESET)"
	@$(CARGO) run --bin $(BINARY) -- essence corpus/medical/single_claim.json --profile staff 2>/dev/null || true

.PHONY: stats
stats: ## Show project statistics
	@echo ""
	@echo -e "  $(C_GOLD)$(C_BOLD)VAJRA$(C_RESET) $(C_DIM)v$(VERSION) ($(GIT_SHA))$(C_RESET)"
	@echo ""
	@echo -e "  $(C_CYAN)Source$(C_RESET)       $$(find . -name '*.rs' -path '*/src/*' | xargs wc -l | tail -1 | awk '{print $$1}') lines"
	@echo -e "  $(C_CYAN)Tests$(C_RESET)        $$(find . -name '*.rs' -path '*/tests/*' | xargs wc -l | tail -1 | awk '{print $$1}') lines"
	@echo -e "  $(C_CYAN)Benchmarks$(C_RESET)   $$(find . -name '*.rs' -path '*/benches/*' | xargs wc -l | tail -1 | awk '{print $$1}') lines"
	@echo -e "  $(C_CYAN)Docs$(C_RESET)         $$(find docs/src -name '*.md' | xargs wc -l | tail -1 | awk '{print $$1}') lines"
	@echo -e "  $(C_CYAN)Crates$(C_RESET)       $$(find . -maxdepth 1 -type d -name 'vajra-*' | wc -l)"
	@echo -e "  $(C_CYAN)Corpus$(C_RESET)       $$(find corpus -type f -not -path '*/golden/*' | wc -l) files, $$(find corpus/golden -type f | wc -l) golden"
	@echo ""
