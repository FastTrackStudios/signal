{
  description = "frame — dev shell with everything the workspace links against";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forAll (pkgs:
        let
          # Same native GUI stack as architect's shell: the architect
          # Dioxus crates link against it even in unit tests.
          linux = with pkgs; lib.optionals stdenv.hostPlatform.isLinux [
            xdotool
            webkitgtk_4_1
            gtk3
            libsoup_3
            glib
            cairo
            pango
            gdk-pixbuf
            atk
            harfbuzz
            dbus
            libappindicator-gtk3
            libayatana-appindicator
            libx11
            libxcb
            # wgpu (the vello parity renders) dlopens the Vulkan loader.
            vulkan-loader
            wayland
            libxkbcommon
          ];
          common = with pkgs; [
            pkg-config
            openssl
            zlib
            sqlite
          ];
          shell = extra: pkgs.mkShell {
            packages = common ++ linux ++ extra;
            # The loader needs the same libraries the linker found, plus the
            # host GPU driver on NixOS (/run/opengl-driver) for wgpu.
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (common ++ linux)
              + pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ":/run/opengl-driver/lib";
          };
        in
        {
          default = shell [ ];
          ci = shell (with pkgs; [ cargo rustc rustfmt clippy rust-analyzer ]);
        });
    };
}
