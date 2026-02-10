{
  description = "parakeet-writer dev env";
  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1"; # tracks nixpkgs unstable branch
    devshell.url = "github:numtide/devshell";
    devenv.url = "https://flakehub.com/f/ramblurr/nix-devenv/*";
  };
  outputs =
    inputs@{
      self,
      devenv,
      devshell,
      ...
    }:
    devenv.lib.mkFlake ./. {
      inherit inputs;
      withOverlays = [
        devshell.overlays.default
        devenv.overlays.default
      ];
      package = pkgs: pkgs.callPackage ./package.nix { };
      devShell =
        pkgs:
        pkgs.devshell.mkShell {
          imports = [ devenv.capsules.base ];
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            pkg-config
            alsa-lib
            openssl
            onnxruntime
            wtype
            wl-clipboard
          ];
          env = [
            {
              name = "ORT_LIB_LOCATION";
              value = "${pkgs.lib.getLib pkgs.onnxruntime}/lib";
            }
            {
              name = "ORT_PREFER_DYNAMIC_LINK";
              value = "1";
            }
            {
              name = "LD_LIBRARY_PATH";
              value = "${pkgs.lib.makeLibraryPath [ pkgs.onnxruntime ]}";
            }
            {
              name = "PKG_CONFIG_PATH";
              value = "${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig";
            }
          ];
        };
    };
}
