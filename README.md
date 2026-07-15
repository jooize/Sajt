# esko.bar

A personal website where the filesystem is the CMS. One folder, synced
through iCloud Drive, is the whole publishing pipeline: drop a file, tag it
`public`, it's published. No database as source of truth, no frontmatter, no
build step — metadata is native metadata (filenames, folder names, file
dates, Finder/Files tags), and the server treats the content folder as
strictly read-only.

## The publishing rule

**The `public` tag on an object publishes everything that belongs to that
object — metadata and contents. Nothing it merely contains.**

- **Tag a file** → you publish that file entirely: its name, its date, and
  its insides (the prose, the bytes, the URL if it's a `link.*` bookmark).
  This holds whether the file is a bare post at the top level or a file
  inside a public folder.
- **Tag a folder** → you publish the folder itself: its name (which becomes
  the URL), its date, its Finder comment (shown as the post description),
  and the fact that a post exists there. The files inside are separate
  objects — each one needs its own `public` tag to be served or listed.

There are no exceptions. A folder post's main document, a listing's intro,
a ` copy [n]` revision snapshot, a `link.*` cite destination — every one of
them is a file, so every one serves only under its own tag. A public folder
whose files are untagged renders as a listing that says how many files are
withheld, and the server log names the file to tag.

Everything fails closed:

- Untagged is never served. `private` beats `public` and hides its whole
  subtree. Every path component down to a file must be `public`.
- A withheld or missing file answers the same 404 — no existence oracle.
- Unpublishing (removing the tag) answers 410 Gone.
- Served images, PDFs, and SVGs pass a metadata-stripping privacy boundary;
  the per-file `public-original` tag is the explicit exact-bytes opt-out.

One caveat worth knowing: Finder does not show a folder's comment at tag
time. The comment is published with the folder, so glance at Get Info
(Cmd-I) before tagging a folder that might carry one.

## Running

Start both processes (see `CLAUDE.md` for development conventions):

```sh
nix develop --command cargo run                        # Rust server on :1234
nix develop --command caddy start --config Caddyfile   # HTTPS proxy on :443
```

Then visit <https://localhost>. To trust Caddy's local CA:
`sudo nix develop --command caddy trust`.

## Documentation

- [`DESIGN.md`](DESIGN.md) — the canonical design summary: content model,
  visibility gate, file-privacy boundary, URL grammar, rendering, theming.
- [`entry-model.md`](entry-model.md) — how files and folders become posts
  (dates, aliases, revisions, name collisions).
- [`post-model.md`](post-model.md) — slugs, kinds, the link axis, folder
  listings, the outbound-scheme guard, media privacy.
- [`PLAN.md`](PLAN.md) — working plan and history.
