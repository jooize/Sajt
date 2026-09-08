# Sajt — build local, ship bytes

**DESIGNED 2026-08-29/30, not yet implemented.** This document is canonical
for the static-publish architecture and the macOS app. It extends
`entry-model.md`, `post-model.md`, and DESIGN.md ("Platform portability",
"The file-privacy boundary"); where the deployment story here conflicts with
older text (rsync content to a dynamic Linux server), this document wins.

Sajt is the engine's name; `esko.bar` is its first instance. Crate
`sajt`, app `Sajt.app`, bundle `bar.esko.Sajt`, client
storage namespace `sajt` (retires the provisional `NS = "site"`).
The word is the Swedish spelling of "site" (it is in SAOL): said aloud it
is exactly what the engine makes, written down it is unmistakably its own.
Name availability verified 2026-09-08: crates.io and Homebrew free; GitHub
carries only unrelated repos; npm holds an obscure `sajt` static-site
generator (not a namespace this project ships to); `sajt.se` and `sajt.dev`
are registered by others, `sajt.app` shows no DNS. The engine was named
StaticDrop from 2026-08-30 to 2026-09-08; that name overclaimed "static"
(only the shipped bytes are) and said nothing about the result.

## The decision

**The site is built entirely on the Mac; the public host stores and serves
bytes and executes none of our logic.** The server was already a pure
function of the content tree — read-only, no accounts, no per-request
state. Sajt finishes the thought: rendering, transcoding, stripping,
verification, embeds, feeds, indexes, and the publish gate all run at build
time, locally. The host is a shelf, not a computer.

This property is called **deadfront** here, after the electrical panel
built so that no live parts are exposed on its face. Concretely:

- **Private files never leave the Mac.** Not gated — absent. A public-host
  compromise cannot leak what was never uploaded.
- **libvips never runs on the internet.** The transcode sandbox exists on
  the authoring machine, where a failure is a bad build, not a breach.
- **The output is auditable.** A build is a directory: hash it, diff it
  against the previous build, inspect it in full before it ships.
- There is no partial version of this. A host that executes any of our
  logic (a template renderer as much as an image decoder) keeps the whole
  class of risk — a process to compromise and an OS to patch. Code may
  later run at the edge (see "Embeds"), but only code whose entire input
  universe is already-public bytes and whose only power is to withhold.

**Rejected:** pushing raw content to a dynamic Linux server (keeps our
code and an image decoder on the internet, with private bytes protected
only by the gate), and any HTTP upload endpoint on the public server
(inverts the read-only invariant the security model rests on). Publishing
is a push from the Mac, never a receive by the web tier.

**The build is now the only place the public/private decision executes**,
so it must prove its work: the build emits a provenance manifest naming,
for every file it intends to upload, the source path and the specific
`public` tag that authorized it, and refuses to upload any file without
that provenance. The app shows the dry run before anything moves:
`12 added / 3 changed / 1 unpublished -> 410`.

## The closure model

The filter space looks infinite; it is not. A tag combination matches
something only if a single post carries all its tags, so the set of
non-empty combinations is the union of the powersets of each post's own
tag set — finite and enumerable.

**The build generates the transitive closure of the UI's own link graph.**
Every filter link a generated page shows (add a co-occurring tag, exclude
one, switch view, step into a date period) is itself generated; iterate to
fixpoint. The invariant that falls out:

> **Every URL the site emits is a real file.**

A no-JS visitor gets the complete filter experience by clicking links,
which is the only way a no-JS visitor navigates anyway.

- A page is generated only if **reachable and distinct**. If every
  `design` post is notable, `/+design/notable` is a 301 to `/+design`,
  not a duplicate page. No-op exclusions collapse the same way.
- Out-of-closure URLs (hand-forged, empty results, orderings beyond the
  rule budget) land on the 404 page, which is the timeline shell: JS
  parses the path, filters, and rewrites to canonical; `<noscript>` shows
  the full timeline with a plain notice.
- The build reports closure size, page counts, and the manifest diff on
  every run, so growth is observable, never a surprise. A depth cap
  (maximum refinements, matching what the UI offers) is a build setting.

## URL grammar

**Canonical scope order: date, then tags, then view** —
`/2026/+design/notable` — when, then what, then how good. Dates first
extends the shipped `/2026/03/` hierarchy and the universal archive
convention; "labels are primary" governs post permalinks (bare labels, no
dates), not filter ordering.

**All orderings are accepted; non-canonical orderings 301.** A redirect
corrects rather than permits: the address bar shows the canonical form
before anyone copies it, so sloppy spellings never propagate. Rejecting
wrong orders with a 404 was considered and refused as the more confusing
behavior. The discipline is on the emit side: **the site only ever
writes, shows, and documents canonical URLs** — redirects are shock
absorbers, not a second grammar. Query spellings of filters are not
accepted as redirect inputs; the site has no legacy to absorb.

These are not workarounds. A URL naming a resource by path is the web's
native design — caches, CDNs, crawlers, and bookmarks all key on paths —
and a filter page is a resource: `/2026/+design/notable` names a specific
stable set of posts. Server-side query processing was the dynamic-era
workaround.

| Today | Becomes | Notes |
|---|---|---|
| `/+design`, `/2026/03/` | real files | as shipped; now composable: `/2026/+design/` |
| `?grade=`, `?favorites` | path variants | author-side content selection, so generated pages; toggles are real links to real files. Multiplier is 2^axes; axes are an editorial budget, pruned by reachable-and-distinct |
| `?q=` | `?search=` | renamed (params are human words). The one irreducible dynamic feature — per-request input, not a resource, so it stays a query forever |
| `?time=` | path | shortest disambiguator: `/2026/03/25/1430/`, seconds (`/143005/`) when needed |
| `?as=jpeg`, `&thumb` | path | `/photo.tif/jpeg`, `/photo.tif/jpeg/thumb`; the bare transcode-only raw URL becomes a styled explainer page (was a 303 with a text body) |
| pagination | date hierarchy | no cursor vocabulary. The head page shows the newest N rows, then "older" links into period pages, granularity chosen by density, empty periods skipped; composes with tags (`/2026/+design/`). Old periods are immutable; a publish touches the head page and the current period only |
| `410`/`301`/CSP | manifest rules | generated; declarative on the host |
| `/saved` | real page, client-filled | reader data — the build cannot know it (the clean test for what remains JS). The build emits the frame (header, explanation, noscript text); JS fills the list from localStorage. No redirect, no special case |

## Hosting: one manifest, adapters translate

The build emits a **host-neutral manifest**: files with hashes, the
redirect map (including unrolled non-canonical orderings), the 410
ledger, and headers (CSP from `security.rs` stays the single source).
Per-host adapters (~50 lines each) render the manifest for a target:
`_redirects`/`_headers` for Pages/Netlify, a Caddyfile snippet, an nginx
map, an S3 sync plan. The manifest is the product; adapters are the only
provider-specific code anywhere, so moving hosts redeploys the same
artifact. Redirects cost lines in one generated file, never files.

The capability ladder — every rung optional, each removes failure cases
for someone:

| Rung | Serves | Host needs |
|---|---|---|
| 1 exact file | everything the site links to | any static host |
| 2 rule map | 301s (orderings to the host's rule budget, deepest-first, shortfall reported), 410s, headers | declarative rule support |
| 3 normalizer | all orderings, forever (~30 lines; sorting is complete at every depth) | any host that runs code; `sajt serve` has it natively |
| 4 fallback shell | forged URLs via 404 + JS; noscript gets the full timeline + notice | a custom 404 page |

**Target: object storage + CDN** (R2 or Bunny), never the provider's
build system (that is where lock-in lives). Upload is ordered — new and
changed files, then index pages, then deletes last — so the worst failure
is a stale link for seconds, never a leak; then verify by re-fetching the
manifest through the CDN and comparing hashes. Credentials live in the
Keychain, scoped to one bucket, write-only. Acknowledged trade-off: a CDN
sees readers (request logs, IPs) — the one remaining argument for a VPS
running the operator's own Caddy with logging off. The VPS is just
another adapter; the choice is values, not architecture.

## Offline and archives

The publish manifest doubles as the offline system:

- **Keep offline (browser).** A button — the reader's explicit gesture —
  registers a service worker that walks the manifest and caches every
  file, then syncs by hash-diff on later visits: incremental site updates
  for free. The button's copy is composed by client-side feature
  detection (the button is JS-only anyway), never UA sniffing:
  `storage.persist()` granted means "kept"; an installed web app drops
  the install hint; otherwise "visiting now and then keeps it fresh."
  Safari evicts all script-writable storage after 7 days without visits
  (visits reset the clock; installed web apps are exempt, bookmarks are
  not; Chrome and Firefox honor `persist()` indefinitely). This is
  accepted as-is — honest copy, no mitigation machinery.
- **Download (disk).** The build emits archives as plain files:
  `esko.bar-everything-20260830T1425Z.zip` — domain, scope, UTC stamp —
  each containing its manifest slice, so an archive is self-describing
  and verifiable. Static, curl-able, evicted by nobody. The browser copy
  is a convenience cache; the zip is the permanent copy. Build-time
  scopes: everything plus a configured list (favorites, notable,
  per-year).
- **Arbitrary scopes** (a filter, a date range, search results, the saved
  list) are assembled client-side from cached files — the only sane home
  for subsets the build cannot enumerate.
- **Reader storage taxonomy.** Cookies never (they ride to the host on
  every request — reader data must not leave the reader's device);
  localStorage for small reader preferences (saved list, typeface);
  Cache Storage for the big offline copy. The offline copy is genuinely
  complete because embeds are cached and served locally — no third-party
  holes.

## Embeds: quotation, attribution, deletion-respect

A locally served embed is a citation: the author's words, attributed,
linked, in our styling with no platform trade dress or JS — and it
protects readers from the platform's trackers (the standard embed widget
ships them there). Anonymous oEmbed fetch of a public endpoint is the
weakest form of terms acceptance; open-federation platforms expect
replication. This posture is defensible, not legal advice; revisit with
counsel near 1.0.

Deletion is honored without requiring the Mac: embeds become addressable
fragments at stable URLs referenced by the pages; a scheduled edge check
re-verifies source liveness, keeps a dead-list, and serves a "this post
was removed" tombstone card in place of the cached quote. That code can
only **withhold** — its entire power is degrading an embed; deadfront is
preserved. Hosts without edge code: a scheduled local rebuild also
satisfies "without undue delay." Either way the public host makes zero
outbound connections; `--embed-check-hours` becomes a rebuild cadence.

## The no-JS floor

- **Search fallback: a generated everything-index** — every post as
  title, date, tags, description. The browser's own find-in-page is the
  engine; the search form's no-JS action lands there, and JS upgrades it
  to live filtering. Search degrades to Ctrl-F over a real page, not to
  nothing.
- **JS-only controls render greyed**, never hidden and never broken:
  `[aria-disabled]` styling (attribute selector — no classes), linked to
  a `#needs-javascript` footer note that `:target` lights up on arrival.
  Tap works on mobile, click on desktop, `title=` on hover; JS un-greys
  the controls on boot. Grey teaches that a capability exists; absence
  would hide it.
- **CSS does ceremony, not architecture.** CSS cannot read the path, the
  query, or typed text, so CSS-driven filtering is rejected outright.
  What it keeps: the `:target` arrival-glow on deep links (used twice —
  deep-linked entries and the needs-JS note). The typeface toggle stays
  as the resident exemplar of a client-side preference done right
  (checkbox hack + localStorage, deliberately not in the URL).
- The noscript footer line: "Saved posts, search, and offline need
  JavaScript; everything else here works without it."

## Components

```
sajt/              one Cargo package, one binary
  src/lib.rs          the engine: scan, entry model, tags, render, clean store, pages
  src/main.rs         the `sajt` command; every lane is a subcommand of it
  src/serve/          `sajt serve`: axum preview and the VPS lane; `grade`, `get`
  src/build/          `sajt build`: walk the closure, emit tree + manifest;
                      `caddyfile` and `verify` adapters
  src/push/           `sajt push`: S3-compatible upload, ordered and verified (planned)

Sajt.app/          Swift, bundle bar.esko.Sajt
  Contents/MacOS/Sajt       SwiftUI shell
  Contents/Helpers/sajt     the Rust binary, spawned as a child
```

**No FFI.** The app spawns the binary as a child process and speaks JSON
lines over stdout, with preview in a WKWebView pointed at a
kernel-assigned loopback port. The crate stays unchanged and CI-testable,
a decoder crash cannot take the app down, and it is bit-for-bit the same
binary in every lane. The app owns: the security-scoped bookmark to the
content folder, FSEvents-driven rescan and live preview, the publish dry
run, and warnings that stay up until dismissed (untagged primary, failed
verify, rejected manifest entry). The Share Extension (planned) writes
into the content folder and nothing more.

**Signing: Developer ID + notarization.** App Sandbox forbids nested
`sandbox-exec`, which would force the vips jail into an XPC service —
real work for a distribution channel not yet needed. The App Store
question stays open until ~1.0, as already recorded in DESIGN.md.

## Implementation order

1. Workspace split (`core`/`serve`/`build`/`push`) — pure refactor, no
   behavior change; `serve` keeps working throughout.
2. Closure builder: URL-space walk, page emission, manifest with
   provenance, build report.
3. Adapters: Caddyfile first (local verification against `serve` as the
   reference implementation), then one CDN target.
4. Push: ordered upload + manifest verification.
5. App shell last; the CLI is fully usable without it.

Open besides the order: icon (the parachute-canopy concepts belonged to the
StaticDrop name and are retired; not yet redesigned for Sajt), registering
`sajt.app` (the one Sajt domain that showed no DNS on 2026-09-08) before
anything public.
