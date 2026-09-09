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
        # Pinned by rust-toolchain.toml (channel + components), never "latest":
        # a build must be reproducible from the repository alone.
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rust
            # Asciidoctor renders .adoc posts as a helper subprocess in its
            # secure safe mode (src/render.rs). Pandoc is the optional helper
            # for .rst/.org/.tex; Markdown renders in-process with comrak and
            # needs neither.
            pkgs.asciidoctor
            pkgs.pandoc
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
