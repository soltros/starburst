{ pkgs ? import <nixpkgs> { } }:
let
  runtime = with pkgs; [ libGL libxkbcommon wayland xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb ];
in pkgs.mkShell {
  packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config openssl gcc cmake par2cmdline-turbo p7zip ] ++ runtime;
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtime;
  RUST_BACKTRACE = "1";
  shellHook = ''
    echo "Starburst • cargo run | ./build.sh | cargo test"
  '';
}
