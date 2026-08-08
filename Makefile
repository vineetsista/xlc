# XLC top-level Makefile — gate dispatcher and build entry points.
#
# Gate exit codes (XLC.md §3):
#   0 = pass    1 = fail (output is the task list)
#   2 = gate unwritten (write it first, Law 12)
#   3 = ran but unmeasurable on this machine (record as unverified, advance)

PHASE := $(shell cat .phase 2>/dev/null || echo 0)

.PHONY: gate gate-all corpus receipt build test clippy

gate:
	@bash scripts/gate.sh $(PHASE)

gate-%:
	@bash scripts/gate.sh $*

# Runs every written gate 0..9. An unwritten gate (exit 2) for a phase beyond
# the current one is skipped; any written gate that fails (1) fails the run.
gate-all:
	@fail=0; \
	for n in 0 1 2 3 4 5 6 7 8 9; do \
		bash scripts/gate.sh $$n; rc=$$?; \
		if [ $$rc -eq 0 ]; then echo "gate($$n): PASS"; \
		elif [ $$rc -eq 2 ]; then \
			if [ $$n -le $(PHASE) ]; then echo "gate($$n): UNWRITTEN (phase <= current — violation of Law 12)"; fail=1; \
			else echo "gate($$n): unwritten (future phase, skipped)"; fi; \
		elif [ $$rc -eq 3 ]; then echo "gate($$n): UNVERIFIED on this machine"; \
		else echo "gate($$n): FAIL"; fail=1; fi; \
	done; exit $$fail

corpus:
	@$(MAKE) -C corpus all

receipt:
	@echo "receipt: not implemented until Phase 3" && exit 2

build:
	cargo build --workspace

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
