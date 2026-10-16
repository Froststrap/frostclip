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
  create-dmg,
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
    ffmpeg-full
    pipewire
    libva
    mesa
    intel-media-driver
    intel-vaapi-driver
    nvidia-vaapi-driver
  ];

  buildInputs = [
    pkg-config
    ffmpeg-full
    clang
    mpv
    just
  ]
  ++ lib.optionals stdenv.isDarwin [
    create-dmg
  ]
  ++ lib.optionals stdenv.isLinux [
    glib
    pipewire
    llvmPackages.libclang
  ];

  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
  LD_LIBRARY_PATH =
    lib.makeLibraryPath runtimeLibs
    + lib.optionalString stdenv.isLinux ":/run/opengl-driver/lib";
  LIBVA_DRIVERS_PATH = lib.makeSearchPath "lib/dri" [
    mesa
    intel-media-driver
    intel-vaapi-driver
    nvidia-vaapi-driver
  ];
}
