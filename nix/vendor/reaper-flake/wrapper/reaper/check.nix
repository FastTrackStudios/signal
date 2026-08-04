# Smoke check: verify the REAPER wrapper produces a working binary.
{ pkgs, self }:

let
  wrapped = self.wrapperModules.reaper.apply {
    pkgs = pkgs;
    extensions.sws = true;
    extensions.reapack = true;
  };
in
pkgs.runCommand "check-reaper-wrapper" { } ''
  # Verify the wrapper script exists and is executable
  test -x ${wrapped}/bin/reaper
  echo "REAPER wrapper binary OK"
  touch $out
''
