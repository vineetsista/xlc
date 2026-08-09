# Security model

XLC has no server, no accounts, and no network calls. That removes most
of the attack surface a web application normally has, and it changes what
"secure" has to mean here. This document says exactly which common
protections apply, which are structurally absent, and what we do instead.

## The standard web-app checklist, answered honestly

| Control | Status | Why |
|---|---|---|
| Session tokens must not sit in `localStorage` | **Not applicable** | There are no sessions and no tokens. Nothing authenticates because there is nothing to authenticate to. `localStorage` holds exactly one kind of value: which findings you marked "intentional", keyed by the SHA-256 of the workbook. It is not a credential, and it never leaves the browser. |
| Authorization must be server-side, never a client-side check | **Not applicable** | There are no roles, no admin, and no privileged actions. There is no server to enforce against — every byte of logic runs in your tab. |
| Two-factor authentication | **Not applicable** | There is no login. |
| Rate limiting on `/login` | **Not applicable** | There is no `/login`, and no endpoint of any kind. The only HTTP requests are the CDN fetching static files. |
| Password strength enforcement | **Not applicable** | There are no passwords. |

If XLC ever grows a paid tier, it will verify license keys **offline**
(signature check, no phone-home), so none of the above changes. The day
this product requires an account is the day it has broken its own thesis.

## What actually matters here, and what we do about it

**A hostile workbook is the real untrusted input.** Findings, formulas,
proofs, sheet and cell names, and diff witnesses all originate in a file
someone else may have written. Every one of those strings reaches the DOM
either through `textContent` or through an escaper that neutralises
`& < > " '`. The live what-if readout was rewritten to use text nodes
exclusively, so the hottest path on the page cannot render markup at all.

**A Content-Security-Policy that proves the privacy claim.** The page
ships `default-src 'none'` with `connect-src 'self'`, no third-party
origins, and no inline script or style. This is not decoration: it is
machine-checkable evidence for Law 1. A reviewer does not have to trust
our claim that nothing is uploaded — the browser will refuse to make the
request. `object-src 'none'`, `base-uri 'none'`, and `form-action 'none'`
close the usual injection escapes, and `referrer: no-referrer` means the
filenames you audit never appear in a referer header.

**Memory safety.** The engine is Rust compiled to WebAssembly, running in
Web Workers inside the browser's sandbox. A malformed workbook produces a
parse error, not a memory-safety bug; the corpus run over 16,167 real
files completes with zero panics. If an engine call does panic, the
worker frees the poisoned session so subsequent calls fail loudly instead
of returning garbage.

**Denial of service against yourself.** A pathological workbook can make
the engine slow. Because the engine lives in workers, that costs you a
spinning computation, never a frozen browser tab.

## What we do not claim

XLC does not protect you from a malicious `.xlsx` doing something to
*Excel*. It does not sandbox macros — it reports that they exist and
excludes them. It does not detect deliberately fraudulent models; it
detects mechanical inconsistency, and it prints the evidence so a human
decides. See `docs/methodology.md`, "What XLC cannot tell you".

## Reporting

Found something? Open an issue at
https://github.com/vineetsista/xlc/issues, or for anything you would
rather not post publicly, say so in the issue and we will move it to
email.
