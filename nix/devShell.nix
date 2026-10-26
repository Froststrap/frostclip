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
  pipewire,
  llvmPackages,
  pkg-config,
  libxcb,
  libxcb-util,
  libxcursor,
  clang,
  glib,
  just,
  ffmpeg-full,
  libva,
  mesa,
  intel-media-driver,
  intel-vaapi-driver,
  nvidia-vaapi-driver,
  inputs,
  mpv,
}:
let
  inherit (inputs) fenix crane;

  toolchain =
    with fenix.packages.${stdenv.system};
    combine [
      latest.toolchain
    ];

  craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
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
    ffmpeg-full
    pipewire
    libva
    mesa
    intel-media-driver
    intel-vaapi-driver
    nvidia-vaapi-driver
  ];
in
craneLib.devShell ({
  meta.license = lib.licenses.unlicense;

  buildInputs = [
    pkg-config
    ffmpeg-full
    clang
    mpv
    just
  ] ++ lib.optionals stdenv.isLinux [
    glib
    pipewire
    llvmPackages.libclang
  ];
}
// lib.optionalAttrs stdenv.isLinux {
  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

  LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs
    + ":/run/opengl-driver/lib";

  LIBVA_DRIVERS_PATH = lib.makeSearchPath "lib/dri" [
    mesa
    intel-media-driver
    intel-vaapi-driver
    nvidia-vaapi-driver
  ];
})
