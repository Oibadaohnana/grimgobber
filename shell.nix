{ pkgs ? import <nixpkgs> { } }:

let
  # What winit and wgpu dlopen at run time: the window system and the GPU
  # loader. Nothing here is linked, so `cargo build` works without it —
  # `cargo run` is what wants LD_LIBRARY_PATH.
  runtimeLibs = with pkgs; [
    vulkan-loader
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];
in
pkgs.mkShell {
  name = "grimgobber";
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    pkg-config
    # The headless compositor `./hidden` opens a window on.
    weston
  ];
  # ALSA is linked, not opened at run time: cpal, under Bevy's audio, finds
  # it through pkg-config, so `cargo build` wants it too.
  buildInputs = runtimeLibs ++ [ pkgs.alsa-lib ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
}
