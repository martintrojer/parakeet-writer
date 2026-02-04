{
  lib,
  rustPlatform,
  pkg-config,
  alsa-lib,
  openssl,
  onnxruntime,
  makeWrapper,
  wtype,
  wl-clipboard,
}:
rustPlatform.buildRustPackage {
  pname = "parakeet-writer";
  version = "0.1.0";

  src = lib.cleanSource ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];

  buildInputs = [
    alsa-lib
    openssl
    onnxruntime
  ];

  env = {
    ORT_LIB_LOCATION = "${lib.getLib onnxruntime}/lib";
    ORT_PREFER_DYNAMIC_LINK = "1";
  };

  postInstall = ''
    wrapProgram $out/bin/parakeet-writer \
      --prefix PATH : ${lib.makeBinPath [ wtype wl-clipboard ]} \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ onnxruntime ]}
  '';

  meta = {
    description = "Minimal push-to-talk transcriber using Parakeet v3";
    homepage = "https://github.com/martintrojer/parakeet-writer";
    license = lib.licenses.mit;
    mainProgram = "parakeet-writer";
    platforms = lib.platforms.linux;
  };
}
