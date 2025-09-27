{
  description = "A Bevy capture development flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self
    , nixpkgs
    , rust-overlay
    , flake-utils
    ,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system;
          inherit overlays;
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Libraries needed at runtime
        libraries = with pkgs; [
          alsa-lib
          vulkan-loader
          libxkbcommon
          libxkbcommon.dev # This provides libxkbcommon-x11.so
          systemd
          wayland
          wayland-protocols
          xorg.libX11
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages =
            with pkgs;
            [
              # Rust toolchain
              rustToolchain

              # Core Bevy Dependencies
              pkg-config

              # Video generation
              ffmpeg-full
            ]
            ++ libraries;

          # Set up library path for runtime
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libraries;

          # Alternative: you can also use shellHook to set the path
          shellHook = ''
          '';
        };
      }
    );
}
