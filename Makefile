SOURCEDIR = spec
BUILDDIR  = spec/_build

# `pds` from PATH if present, else a quiet `cargo run` against an in-tree
# crate — from cli/ so rustup honours cli/rust-toolchain.toml (cwd-based).
# pds is not installed in this repo yet; the gate targets fall back to the
# raw builders below when pds is absent.
PDS = $(shell command -v pds 2>/dev/null)

.PHONY: html strict needs serve clean

html:  ## Build the HTML spec (NOT the gate — ADR_0017)
	uv run sphinx-build -b html "$(SOURCEDIR)" "$(BUILDDIR)/html"

strict:  ## Strict gate — every spec mutation must pass this (ADR_0007, ADR_0017)
	@if [ -n "$(PDS)" ]; then \
		$(PDS) check; \
	else \
		uv run sphinx-build -W -b html "$(SOURCEDIR)" "$(BUILDDIR)/html"; \
	fi

needs:  ## Build a fresh needs.json for querying (ADR_0006)
	@if [ -n "$(PDS)" ]; then \
		$(PDS) build; \
	else \
		mkdir -p "$(BUILDDIR)/needs"; \
		ubc build needs --outpath "$(BUILDDIR)/needs/needs.json"; \
	fi

serve:  ## Live preview with auto-rebuild (port 8000)
	uv run sphinx-autobuild "$(SOURCEDIR)" "$(BUILDDIR)/html"

clean:  ## Remove build artifacts
	rm -rf "$(BUILDDIR)"
