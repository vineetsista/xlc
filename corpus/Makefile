# XLC corpus — pinned sources, reproducible from a clean checkout.
#
# `make -C corpus all` fetches, verifies, extracts, and filters everything.
# Raw archives live in corpus/raw/ (gitignored). Committed artifacts are
# manifest.json + subset/ (the 500-workbook regression set) and census.json.
#
# Licenses: FUSE is CC-BY-4.0 (redistribution OK with attribution).
# SpreadsheetBench is CC-BY-SA-4.0 — TESTING ONLY, never vendored into any
# shipped artifact (XLC.md Part IV).

RAW := raw
WORK := work

# --- FUSE spreadsheet corpus (Zenodo record 581678, DOI 10.5281/zenodo.581678)
FUSE_URL := https://zenodo.org/api/records/581678/files/fuse.zip/content
FUSE_MD5 := 13e955c44f0b77d1c36088c0bbb3366d
FUSE_SHA256 := 4f9126bd83aa27118366b2b16b3b378d3bdb27b27728337b392e48c0ef3d93bb
FUSE_ZIP := $(RAW)/fuse.zip

# --- SpreadsheetBench (RUCKBReasoning/SpreadsheetBench, main branch)
# NOTE: XLC.md names data/all_data_912.tar.gz; the repo has since renamed it.
# Corrected 2026-08-08 (docs/decisions.md).
SB_URL := https://raw.githubusercontent.com/RUCKBReasoning/SpreadsheetBench/main/data/spreadsheetbench_912_v0.1.tar.gz
SB_SHA256 := 9cf7228b54f1edcdd4b372eb736774adf29cb4f804c9920229bac6c154833399
SB_TAR := $(RAW)/spreadsheetbench_912_v0.1.tar.gz

.PHONY: all fetch verify-raw extract filter subset clean

all: fetch verify-raw extract filter

fetch: $(FUSE_ZIP) $(SB_TAR)

$(FUSE_ZIP):
	mkdir -p $(RAW)
	curl -L -C - --retry 10 --retry-delay 15 --retry-all-errors -o $@ "$(FUSE_URL)"

$(SB_TAR):
	mkdir -p $(RAW)
	curl -L -C - --retry 5 --retry-delay 10 --retry-all-errors -o $@ "$(SB_URL)"

# Verifies whatever raw archives are present against their pins.
# Absent archives are fine (they are fetched on demand); a present archive
# that mismatches its pin is a hard failure.
verify-raw:
	@ok=0; \
	if [ -f $(FUSE_ZIP) ]; then \
		if [ "$(FUSE_SHA256)" = "PENDING" ]; then echo "FUSE sha256 unpinned"; ok=1; \
		elif echo "$(FUSE_SHA256)  $(FUSE_ZIP)" | sha256sum -c --quiet -; then echo "fuse.zip: sha256 ok"; \
		else echo "fuse.zip: SHA256 MISMATCH"; ok=1; fi; \
	fi; \
	if [ -f $(SB_TAR) ]; then \
		if echo "$(SB_SHA256)  $(SB_TAR)" | sha256sum -c --quiet -; then echo "spreadsheetbench: sha256 ok"; \
		else echo "spreadsheetbench: SHA256 MISMATCH"; ok=1; fi; \
	fi; \
	exit $$ok

# fuse.zip wraps a 140-part split 7z (FUSE.7z.001..140) — discovered on
# first extraction, not documented on Zenodo (docs/decisions.md). We stream
# each part out of the zip in order into one FUSE.7z, then extract with
# py7zr in a local venv (no system 7z assumed, no sudo needed).
extract: fetch verify-raw
	mkdir -p $(WORK)
	@if [ ! -f $(WORK)/venv/bin/python ]; then \
		python3 -m venv $(WORK)/venv && $(WORK)/venv/bin/pip install -q py7zr; \
	fi
	@if [ ! -f $(WORK)/FUSE.7z ]; then \
		echo "reassembling FUSE.7z from 140 split parts..."; \
		rm -f $(WORK)/FUSE.7z.partial; \
		for i in $$(seq -w 1 140); do unzip -p $(FUSE_ZIP) FUSE.7z.$$i >> $(WORK)/FUSE.7z.partial; done; \
		mv $(WORK)/FUSE.7z.partial $(WORK)/FUSE.7z; \
	fi
	@if [ ! -d $(WORK)/fuse ]; then \
		echo "extracting FUSE.7z (large, takes a while)..."; \
		$(WORK)/venv/bin/python -c "import py7zr; py7zr.SevenZipFile('$(WORK)/FUSE.7z').extractall('$(WORK)/fuse.partial')"; \
		mv $(WORK)/fuse.partial $(WORK)/fuse; \
	fi
	@if [ ! -d $(WORK)/spreadsheetbench ]; then \
		mkdir -p $(WORK)/spreadsheetbench; \
		tar xzf $(SB_TAR) -C $(WORK)/spreadsheetbench; \
	fi

# Filter FUSE to valid OOXML workbooks with >=1 formula cell.
# Produces work/filtered.json: one record per candidate with path, sha256,
# formula-cell count, and load status. Implemented by `xlc corpus-filter`.
filter: extract
	cargo build --release -p xlc-cli
	../target/release/xlc corpus-filter $(WORK)/fuse --out $(WORK)/filtered.json

# Deterministic 500-workbook regression subset: valid + >=10 formula cells,
# ordered by content sha256 ascending, first 500. See docs/decisions.md.
subset:
	../target/release/xlc corpus-subset $(WORK)/filtered.json --n 500 \
		--min-formulas 10 --out-dir subset --manifest manifest.json

clean:
	rm -rf $(WORK)
