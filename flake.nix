{
  description = "Grimgobber: a top-down co-op ghost hunt, and a headless server to play it together";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  # Builds the dependencies as a derivation of their own. Bevy and the rest
  # are most of the build, and they only change when Cargo.lock does -- so a
  # pin bump that only touched the game's own code reuses them from the store
  # instead of compiling the engine again.
  inputs.crane.url = "github:ipetkov/crane";

  outputs =
    {
      self,
      nixpkgs,
      crane,
    }:
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
          craneLib = crane.mkLib pkgs;

          # The whole tree, not craneLib.cleanCargoSource: sound.rs
          # include_bytes! the clips under sounds/. buildDepsOnly swaps in a
          # dummy source of its own, so this does not tie the dependency build
          # to the game's code.
          common = {
            pname = "grimgobber";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            inherit src;
            strictDeps = true;
            meta.mainProgram = "grimgobber";
          };

          # Each build pairs with its own dependency derivation: the features
          # differ, so the compiled crates do too. Both also compile the test
          # profile's dependencies, so the tests reuse them.
          build =
            args:
            craneLib.buildPackage (
              args // { cargoArtifacts = craneLib.buildDepsOnly (builtins.removeAttrs args [ "meta" ]); }
            );

          # Headless: no `client` feature, so no wgpu, winit, Wayland, X11 or
          # audio is compiled or linked (see Cargo.toml). This is what runs at
          # grim.buggly.de.
          grimgobber-server = build (
            common
            // {
              cargoExtraArgs = "--locked --no-default-features";
            }
          );

          # The game. ALSA and Wayland's client library are linked; the GPU
          # loader and X11 are opened at run time, so the wrapper below names
          # them.
          grimgobber-client = build (
            common
            // {
              nativeBuildInputs = [ pkgs.pkg-config ];
              buildInputs = [
                pkgs.alsa-lib
                pkgs.wayland
              ];
            }
          );

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
            paths = [ grimgobber-client ];
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
