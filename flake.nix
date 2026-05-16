{
  description = "Pares Radix — unified platform: Svelte UI + Rust agent crates + Tauri desktop shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      # Read version from Cargo.toml workspace — single source of truth
      cargoVersion = let
        cargo = builtins.readFile ./Cargo.toml;
        lines = builtins.filter (l: builtins.match ''version = ".*"'' l != null)
          (nixpkgs.lib.splitString "\n" cargo);
        raw = builtins.head lines;
      in builtins.head (builtins.match ''.*"(.*)".*'' raw);

      # Official Microsoft ONNX Runtime release — has both .so and headers.
      # fetchurl is a fixed-output derivation so it ALWAYS gets network access
      # regardless of sandbox settings. This is the correct Nix pattern.
      onnxruntime = { pkgs }: pkgs.stdenvNoCC.mkDerivation {
        pname = "onnxruntime-prebuilt";
        version = "1.23.0";
        src = pkgs.fetchurl {
          url = "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/onnxruntime-linux-x64-1.23.0.tgz";
          hash = "sha256-tt7qfy4iwQwEMBnylKDqTSpsCuUqAJw0hHZA23XsVYA=";
        };
        sourceRoot = ".";
        installPhase = ''
          mkdir -p $out/lib $out/include
          cp -a onnxruntime-linux-x64-1.23.0/lib/* $out/lib/
          cp -a onnxruntime-linux-x64-1.23.0/include/* $out/include/
        '';
      };

      # Shared build config
      mkRadixBuild = pkgs: extraAttrs:
        let ort = onnxruntime { inherit pkgs; };
        in pkgs.rustPlatform.buildRustPackage ({
        pname = "pares-radix";
        version = cargoVersion;
        src = pkgs.lib.cleanSource ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          allowBuiltinFetchGit = true;
        };

        # Network for git deps
        __noChroot = true;

        doCheck = false;

        nativeBuildInputs = with pkgs; [ pkg-config cmake makeWrapper ];
        buildInputs = with pkgs; [
          openssl stdenv.cc.cc.lib glib pango cairo gdk-pixbuf atk gtk3
          graphene webkitgtk_4_1 libsoup_3
        ];

        # ort-sys links ONNX Runtime. With ORT_LIB_LOCATION it uses our
        # prefetched .so. ORT_PREFER_DYNAMIC_LINK=1 tells it to link dynamically
        # (default is static, which fails with .so files).
        ORT_LIB_LOCATION = "${ort}/lib";
        ORT_PREFER_DYNAMIC_LINK = "1";

        # Runtime: ort dlopen needs to find the .so
        postInstall = ''
          wrapProgram $out/bin/pares-radix \
            --set ORT_DYLIB_PATH "${ort}/lib/libonnxruntime.so"
        '';

        meta = {
          homepage = "https://github.com/plures/pares-radix";
          license = pkgs.lib.licenses.bsl11;
          mainProgram = "pares-radix";
        };
      } // extraAttrs);
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; config.allowUnfree = true; };
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
        };

        cliPkg = mkRadixBuild pkgs {
          pname = "pares-radix";
          cargoBuildFlags = [ "-p" "pares-radix-cli" ];
          meta.description = "Pares Radix — headless AI agent daemon";
        };

        tauriPkg = mkRadixBuild pkgs {
          pname = "pares-radix-desktop";
          cargoBuildFlags = [ "-p" "pares-radix" ];
          nativeBuildInputs = with pkgs; [ pkg-config cmake makeWrapper nodejs_22 ];
          meta.description = "Pares Radix — Tauri 2 desktop shell with Svelte UI";

          preBuild = ''
            npm ci --ignore-scripts
            npm run build
          '';
        };
      in
      {
        packages.default = cliPkg;
        packages.cli = cliPkg;
        packages.desktop = tauriPkg;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust pkg-config openssl cmake stdenv.cc.cc.lib cargo-watch
            glib pango cairo gdk-pixbuf atk gtk3 graphene
            webkitgtk_4_1 libsoup_3
            nodejs_22
          ];
        };
      }
    ) // {
      overlays.default = final: prev:
        let ort = onnxruntime { pkgs = final; };
        in {
        pares-radix = (mkRadixBuild final {
          cargoBuildFlags = [ "-p" "pares-radix-cli" ];
        });
        pares-radix-desktop = (mkRadixBuild final {
          pname = "pares-radix-desktop";
          cargoBuildFlags = [ "-p" "pares-radix" ];
          nativeBuildInputs = with final; [ pkg-config cmake makeWrapper nodejs_22 ];
          preBuild = ''
            npm ci --ignore-scripts
            npm run build
          '';
        });
      };

      # NixOS module — headless agent daemon service
      nixosModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.pares-radix;
        in
        {
          options.services.pares-radix = {
            enable = lib.mkEnableOption "Pares Radix AI agent daemon";

            package = lib.mkOption {
              type = lib.types.package;
              default = pkgs.pares-radix;
              defaultText = lib.literalExpression "pkgs.pares-radix";
              description = "The pares-radix package to use. Requires the pares-radix overlay.";
            };

            user = lib.mkOption {
              type = lib.types.str;
              default = "pares-radix";
              description = "User account under which the service runs.";
            };

            group = lib.mkOption {
              type = lib.types.str;
              default = "pares-radix";
              description = "Group under which the service runs.";
            };

            dataDir = lib.mkOption {
              type = lib.types.path;
              default = "/var/lib/pares-radix";
              description = "Directory for PluresDB storage and Copilot auth cache.";
            };

            copilot = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Use GitHub Copilot OAuth device flow for LLM access.";
            };

            model = lib.mkOption {
              type = lib.types.str;
              default = "gpt-4.1";
              description = "Conscious model (80% of traffic).";
            };

            deepModel = lib.mkOption {
              type = lib.types.str;
              default = "claude-opus-4.6";
              description = "Deep model for low-confidence escalation.";
            };

            telegramTokenFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Path to file containing the Telegram bot token.";
            };

            braveApiKeyFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Path to file containing the Brave Search API key.";
            };

            syncTopicKey = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "32-byte Hyperswarm sync topic key in hex.";
            };

            syncSharedKeyFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Path to file containing shared SEA key for sync.";
            };

            systemPromptFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Path to a system prompt file.";
            };

            createUser = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Whether to create the service user.";
            };

            bitnetModelPath = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "Path to a BitNet model file for local inference.";
            };

            extraFlags = lib.mkOption {
              type = lib.types.listOf lib.types.str;
              default = [];
              description = "Additional command-line flags.";
            };
          };

          config = lib.mkIf cfg.enable {
            assertions = [
              {
                assertion = cfg.telegramTokenFile != null;
                message = "services.pares-radix.telegramTokenFile must be set.";
              }
              {
                assertion = cfg.syncTopicKey == null || cfg.syncSharedKeyFile != null;
                message = "services.pares-radix.syncSharedKeyFile must be set when syncTopicKey is configured.";
              }
            ];

            users.users.${cfg.user} = lib.mkIf cfg.createUser {
              isSystemUser = true;
              group = cfg.group;
              home = cfg.dataDir;
              createHome = true;
            };

            users.groups.${cfg.group} = lib.mkIf cfg.createUser {};

            systemd.services.pares-radix = {
              description = "Pares Radix — AI Agent Daemon";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" ];
              wants = [ "network-online.target" ];

              environment = {
                RUST_LOG = "info";
                HOME = cfg.dataDir;
              };

              serviceConfig = {
                Type = "notify";
                NotifyAccess = "main";
                WatchdogSec = 30;
                User = cfg.user;
                Group = cfg.group;
                WorkingDirectory = cfg.dataDir;
                Restart = "on-failure";
                RestartSec = 10;
                NoNewPrivileges = lib.mkIf cfg.createUser true;
                ProtectSystem = lib.mkIf cfg.createUser "strict";
                ProtectHome = lib.mkIf cfg.createUser true;
                ReadWritePaths = [ cfg.dataDir ];
                PrivateTmp = true;
              };

              script =
                let
                  escapedTelegramTokenFile = lib.escapeShellArg (toString cfg.telegramTokenFile);
                  copilotArg = if cfg.copilot then "--copilot" else "";
                  modelArg = "--model ${cfg.model} --deep-model ${cfg.deepModel}";
                  promptArg = if cfg.systemPromptFile != null
                    then "--system-prompt ${cfg.systemPromptFile}"
                    else "";
                  syncArg = if cfg.syncTopicKey != null
                    then "--sync-topic-key ${cfg.syncTopicKey}"
                    else "";
                  bitnetArg = if cfg.bitnetModelPath != null
                    then "--bitnet-model-path ${cfg.bitnetModelPath}"
                    else "";
                  escapedBraveApiKeyFile = if cfg.braveApiKeyFile != null
                    then lib.escapeShellArg (toString cfg.braveApiKeyFile)
                    else null;
                  escapedSyncSharedKeyFile = if cfg.syncSharedKeyFile != null
                    then lib.escapeShellArg (toString cfg.syncSharedKeyFile)
                    else null;
                  telegramTokenExport = "export PARES_TELEGRAM_TOKEN=\"$(tr -d '\\r\\n' < ${escapedTelegramTokenFile})\"";
                  braveApiKeyExport = if cfg.braveApiKeyFile != null
                    then "export BRAVE_API_KEY=\"$(tr -d '\\r\\n' < ${escapedBraveApiKeyFile})\""
                    else "";
                  syncSharedKeyExport = if cfg.syncSharedKeyFile != null
                    then "export PARES_SYNC_SHARED_KEY=\"$(tr -d '\\r\\n' < ${escapedSyncSharedKeyFile})\""
                    else "";
                  extraArgs = lib.concatStringsSep " " cfg.extraFlags;
                in
                ''
                  ${telegramTokenExport}
                  ${braveApiKeyExport}
                  ${syncSharedKeyExport}

                  exec ${cfg.package}/bin/pares-radix serve \
                    ${copilotArg} \
                    ${modelArg} \
                    ${promptArg} \
                    ${syncArg} \
                    ${bitnetArg} \
                    ${extraArgs}
                '';
            };
          };
        };
    };
}
