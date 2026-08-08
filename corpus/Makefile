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
FUSE_SHA256 := PENDING
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

extract: fetch verify-raw
	mkdir -p $(WORK)
	@if [ ! -d $(WORK)/fuse ]; then \
		echo "extracting fuse.zip (this is large)..."; \
		unzip -q -n $(FUSE_ZIP) -d $(WORK)/fuse; \
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
