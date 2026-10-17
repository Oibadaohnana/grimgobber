{
  description = "Bank Bobbery: a top-down heist, and a headless server to play it together";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      eachSystem = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      src = nixpkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type: !(type == "directory" && builtins.elem (baseNameOf path) [ "target" "result" ]);
      };

      packagesFor =
        pkgs:
        let
          # One build. ALSA and Wayland's client library are linked; the GPU
          # loader and X11 are opened at run time, so only the player's
          # wrapper names them and the server's closure has no GPU stack.
          grimgobber-server = pkgs.rustPlatform.buildRustPackage {
            pname = "grimgobber";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            inherit src;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [
              pkgs.alsa-lib
              pkgs.wayland
            ];
            meta.mainProgram = "grimgobber";
          };

          runtimeLibs = with pkgs; [
            vulkan-loader
            libxkbcommon
            wayland
            libx11
            libxcursor
            libxi
            libxrandr
          ];

          grimgobber = pkgs.symlinkJoin {
            name = "grimgobber";
            paths = [ grimgobber-server ];
            nativeBuildInputs = [ pkgs.makeWrapper ];
            postBuild = ''
              wrapProgram "$out/bin/grimgobber" \
                --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
            '';
            meta.mainProgram = "grimgobber";
          };
        in
        {
          inherit grimgobber grimgobber-server;
          default = grimgobber;
        };
    in
    {
      packages = eachSystem packagesFor;
    };
}
