{
  pkgs,
  lib,
  libx11,
  libice,
  libsm,
  libxi,
  libxrandr,
  stdenv,
  expat,
  fontconfig,
  freetype,
  libGL,
  vulkan-loader,
  wayland,
  libxkbcommon,
  pkg-config,
  libxcb,
  libxcb-util,
  libxcursor,
  clang,
  glib,
  just,
  create-dmg,
  ffmpeg-full,
  inputs,
}:
let
  inherit (inputs) fenix crane;

  toolchain =
    with fenix.packages.${stdenv.system};
    combine [
      latest.toolchain
    ];

  craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
in
craneLib.devShell rec {
  meta.license = lib.licenses.unlicense;
  runtimeLibs = lib.optionals stdenv.isLinux [
    expat
    fontconfig
    freetype
    libGL
    vulkan-loader
    wayland
    libxkbcommon

    libx11
    libice
    libsm
    libxi
    libxrandr
    libxcursor
    libxcb
    libxcb-util
  ];

  buildInputs = [
    pkg-config
    ffmpeg-full
    clang
    just
  ]
  ++ lib.optionals stdenv.isDarwin [
    create-dmg
  ]
  ++ lib.optionals stdenv.isLinux [
    glib
  ];

  LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
}
