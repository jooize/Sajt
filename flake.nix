{
  description = "Sajt: publishing engine where the filesystem is the CMS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        inherit (pkgs) lib;
        # Pinned by rust-toolchain.toml (channel + components), never "latest":
        # a build must be reproducible from the repository alone.
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        cargoToml = lib.importTOML ./Cargo.toml;

        # Exactly what the compiler reads, and nothing else. The site's own
        # content, the build output, the design documents and the agent
        # scratch directories all live beside these files; if they entered
        # the derivation, editing a document would change the store path of
        # the binary, and personal site data would be copied into the Nix
        # store on every build. Nothing here embeds an asset at compile time
        # (src/assets.rs generates its CSS and JS from consts), so src/ plus
        # the manifests is the whole of it.
        source = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./rust-toolchain.toml
            ./build.rs
            ./src
          ];
        };

        # Linux releases are statically linked against musl: one file that
        # runs on any distribution, with no glibc version to match and no
        # loader to find. Everything the crate binds is either pure Rust or a
        # system facility reached through a syscall (the -sys crates in
        # Cargo.lock are core-foundation, fsevent, kqueue, inotify, linux-raw,
        # windows and js), so nothing has to be taught to link statically.
        # macOS gets the ordinary build: the system frameworks are the ABI
        # there, and static linking against them is not a thing Apple offers.
        pkgsStatic = pkgs.pkgsStatic;
        rustStatic = rust.override {
          targets = [ pkgsStatic.stdenv.hostPlatform.rust.rustcTarget ];
        };
        rustPlatform =
          if pkgs.stdenv.isLinux then
            pkgsStatic.makeRustPlatform { cargo = rustStatic; rustc = rustStatic; }
          else
            pkgs.makeRustPlatform { cargo = rust; rustc = rust; };

        # Helper programs the engine looks for on PATH at run time:
        # asciidoctor renders .adoc in its secure safe mode (src/render.rs
        # run_helper), vips transcodes the image formats pure Rust cannot
        # strip, and bwrap is the Linux transcode sandbox (src/media.rs
        # find_on_path). macOS reaches its sandbox at the fixed path
        # /usr/bin/sandbox-exec, so nothing is needed for that.
        helpers = [ pkgs.asciidoctor pkgs.vips ]
          ++ lib.optionals pkgs.stdenv.isLinux [ pkgs.bubblewrap ];

        sajt-unwrapped = rustPlatform.buildRustPackage {
          pname = "sajt";
          inherit (cargoToml.package) version;
          src = source;

          cargoLock.lockFile = ./Cargo.lock;

          # The commit this build came from, read by build.rs and printed by
          # `sajt --version`. A dirty tree says so, so a binary built from
          # uncommitted work can never be mistaken for a release.
          env.SAJT_COMMIT = self.rev or self.dirtyRev or "unknown";

          # The test suite needs a filesystem that carries extended
          # attributes and the helper programs on PATH (the visibility gates
          # write real tags, the render tests call asciidoctor). The Nix
          # sandbox has neither, so tests run in the devshell instead, on
          # every push (ci.yml) and again in the release workflow's test job
          # before anything is built for release.
          doCheck = false;

          # rustc records source paths for panic locations, and dependencies
          # compile from under the Nix build directory. Linux's sandbox
          # always calls that /build, while macOS's is random per build, so
          # without this the same commit yields different bytes on the two
          # platforms and even between two macOS builds. buildRustPackage
          # sets no RUSTFLAGS of its own for a release build, so appending is
          # safe.
          #
          # On macOS the toolchain also hands the linker nixpkgs' libiconv
          # (rust-overlay propagates it for the Darwin standard library) and
          # its libintl, and ld64 records a load command for each even though
          # the binary references no symbol from either -- `nm -u` lists none.
          # Two store paths would then have to travel with a release binary.
          # -dead_strip_dylibs is the linker's own answer: drop the load
          # command for a library nothing is used from. If iconv ever does
          # become a real dependency the load command stays, and the
          # portability check below turns that into a failed build rather
          # than a broken download.
          preBuild = ''
            export RUSTFLAGS="''${RUSTFLAGS-} --remap-path-prefix $NIX_BUILD_TOP=/build"
          '' + lib.optionalString pkgs.stdenv.isDarwin ''
            export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-dead_strip_dylibs"
          '';

          # A binary that only runs inside a Nix store is not a release.
          # Refuse to produce one rather than discover it after publishing.
          # This runs in postFixup, after the install-name rewriting and the
          # ad-hoc signature that the Darwin stdenv applies, so it inspects
          # the bytes that actually ship.
          postFixup = ''
            bin="$out/bin/sajt"
          '' + (if pkgs.stdenv.isLinux then ''
            readelf="''${READELF:-readelf}"
            if "$readelf" -l "$bin" | grep -q INTERP; then
              echo "ERROR: $bin asks for a dynamic loader; a release binary must be static" >&2
              "$readelf" -l "$bin" | grep INTERP >&2
              exit 1
            fi
            if "$readelf" -d "$bin" 2>/dev/null | grep -q NEEDED; then
              echo "ERROR: $bin has shared library dependencies; a release binary must be static" >&2
              "$readelf" -d "$bin" | grep NEEDED >&2
              exit 1
            fi
            echo "portability check: statically linked, no loader, no shared libraries"
          '' else ''
            otool="''${OTOOL:-otool}"
            libs="$("$otool" -L "$bin" | tail -n +2 | awk '{ print $1 }')"
            bad="$(printf '%s\n' "$libs" | grep -v '^/usr/lib/' | grep -v '^/System/Library/' || true)"
            if [ -n "$bad" ]; then
              echo "ERROR: $bin links libraries outside the system:" >&2
              printf '%s\n' "$bad" >&2
              echo "A release binary must reference only /usr/lib and /System/Library." >&2
              exit 1
            fi
            echo "portability check: system libraries only"
            printf '%s\n' "$libs"
          '') + ''
            # The workflow runs this too, and compares what it prints against
            # the commit being released; fail here if it cannot even run.
            "$bin" --version
          '';

          meta = {
            inherit (cargoToml.package) description;
            homepage = cargoToml.package.repository;
            license = with lib.licenses; [ mit asl20 ];
            mainProgram = "sajt";
          };
        };

        # What a Nix user gets: the same binary, with the helper programs it
        # looks for already on its PATH, so `nix run` works without a word of
        # setup. The wrapper only writes a small script beside a symlink to
        # the binary above; it never rebuilds it.
        sajt = pkgs.symlinkJoin {
          name = "sajt-${cargoToml.package.version}";
          paths = [ sajt-unwrapped ];
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postBuild = ''
            wrapProgram $out/bin/sajt \
              --prefix PATH : ${lib.makeBinPath helpers}
          '';
          meta = sajt-unwrapped.meta;
        };
      in {
        packages = {
          inherit sajt sajt-unwrapped;
          default = sajt;
        };

        apps.default = {
          type = "app";
          program = "${sajt}/bin/sajt";
          meta = { inherit (sajt-unwrapped.meta) description; };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rust
            # Asciidoctor renders .adoc posts as a helper subprocess in its
            # secure safe mode (src/render.rs); Markdown renders in-process
            # with comrak and needs no helper.
            pkgs.asciidoctor
            # libvips: transcodes formats we cannot segment-strip in pure Rust
            # (HEIC/HEIF/TIFF/AVIF/GIF/BMP) into a clean JPEG, and generates
            # gallery thumbnails. Runs as a sandboxed subprocess (post-model.md
            # §8, C8b/C8c). Pulls libheif for HEIC decode.
            pkgs.vips
            pkgs.cargo-watch
            pkgs.caddy
          ]
          # bubblewrap sandboxes the vips transcode subprocess on Linux (the
          # counterpart of macOS sandbox-exec; transcoding fails closed without
          # it), so a Linux dev shell or server must carry it.
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.bubblewrap ];
        };
      }
    );
}
