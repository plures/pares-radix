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

      # Shared build config for both CLI and Tauri
      mkRadixBuild = pkgs: extraAttrs: pkgs.rustPlatform.buildRustPackage ({
        pname = "pares-radix";
        version = cargoVersion;
        src = pkgs.lib.cleanSource ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          allowBuiltinFetchGit = true;
        };

        # Network access for git deps and ort-sys ONNX download
        __noChroot = true;

        doCheck = false;

        nativeBuildInputs = with pkgs; [ pkg-config cmake ];
        buildInputs = with pkgs; [
          openssl stdenv.cc.cc.lib glib pango cairo gdk-pixbuf atk gtk3
          graphene webkitgtk_4_1 libsoup_3
        ];

        # ort-sys downloads its own ONNX Runtime binary at build time.
        # __noChroot gives it network access. Do NOT set ORT_LIB_LOCATION —
        # let ort-sys use its own download + bundling logic.

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
          nativeBuildInputs = with pkgs; [ pkg-config cmake nodejs_22 ];
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
      overlays.default = final: prev: {
        pares-radix = (mkRadixBuild final {
          cargoBuildFlags = [ "-p" "pares-radix-cli" ];
        });
        pares-radix-desktop = (mkRadixBuild final {
          pname = "pares-radix-desktop";
          cargoBuildFlags = [ "-p" "pares-radix" ];
          nativeBuildInputs = with final; [ pkg-config cmake nodejs_22 ];
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
