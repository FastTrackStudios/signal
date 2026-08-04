# Generate a macOS .icns icon from the FTS SVG template.
#
# Uses librsvg to render PNGs at all required sizes, then iconutil to
# assemble the .icns bundle.
{
  lib,
  stdenv,
  librsvg,
  iconSvg ? ../assets/icon.svg,
}:

assert stdenv.hostPlatform.isDarwin;

stdenv.mkDerivation {
  pname = "fts-reaper-icon";
  version = "1.0";

  src = iconSvg;
  dontUnpack = true;

  nativeBuildInputs = [ librsvg ];

  # iconutil is a macOS system tool
  __noChroot = true;

  buildPhase = ''
    mkdir -p icon.iconset

    # Render at all required sizes for macOS iconset
    for size in 16 32 64 128 256 512 1024; do
      rsvg-convert -w $size -h $size $src -o icon_''${size}.png
    done

    cp icon_16.png   icon.iconset/icon_16x16.png
    cp icon_32.png   icon.iconset/icon_16x16@2x.png
    cp icon_32.png   icon.iconset/icon_32x32.png
    cp icon_64.png   icon.iconset/icon_32x32@2x.png
    cp icon_128.png  icon.iconset/icon_128x128.png
    cp icon_256.png  icon.iconset/icon_128x128@2x.png
    cp icon_256.png  icon.iconset/icon_256x256.png
    cp icon_512.png  icon.iconset/icon_256x256@2x.png
    cp icon_512.png  icon.iconset/icon_512x512.png
    cp icon_1024.png icon.iconset/icon_512x512@2x.png

    /usr/bin/iconutil -c icns icon.iconset -o fts-reaper.icns
  '';

  installPhase = ''
    mkdir -p $out
    cp fts-reaper.icns $out/fts-reaper.icns
  '';

  meta = {
    description = "FastTrack REAPER application icon";
    platforms = [ "x86_64-darwin" "aarch64-darwin" ];
  };
}
