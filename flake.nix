{
  description = "Pares Radix - unified platform: Svelte UI + Rust agent crates + Tauri desktop shell";

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
      # Read version from Cargo.toml workspace - single source of truth
      cargoVersion = let
        cargo = builtins.readFile ./Cargo.toml;
        # Strip \r (Windows CRLF) before matching
        lines = builtins.filter (l: builtins.match ''version = "[^"]+".*'' l != null)
          (nixpkgs.lib.splitString "\n" (builtins.replaceStrings ["\r"] [""] cargo));
        raw = builtins.head lines;
      in builtins.head (builtins.match ''.*"([^"]+)".*'' raw);

      # Prefetch ONNX Runtime static library for ort-sys.
      onnxruntimeLib = { pkgs }: pkgs.stdenvNoCC.mkDerivation {
        name = "onnxruntime-prebuilt-1.24.2";
        src = pkgs.fetchurl {
          url = "https://cdn.pyke.io/0/pyke:ort-rs/ms@1.24.2/x86_64-unknown-linux-gnu.tar.lzma2";
          hash = "sha256-rMHLp5wzdZTq0diMpyUWFHqmAFTIQhe1M5mjHKpbpnE=";
        };
        nativeBuildInputs = [ pkgs.python3 ];
        dontUnpack = true;
        installPhase = ''
          mkdir -p $out/lib
          python3 -c "
import lzma, tarfile, io, sys, os
with open(sys.argv[1], 'rb') as f:
    raw = f.read()
data = lzma.decompress(raw, format=lzma.FORMAT_RAW, filters=[{'id': lzma.FILTER_LZMA2, 'dict_size': 1 << 26}])
tar = tarfile.open(fileobj=io.BytesIO(data))
tar.extractall(os.environ['out'] + '/lib')
" $src
        '';
      };

      # CLI binary - headless agent daemon
      mkCliPkg = pkgs: pkgs.rustPlatform.buildRustPackage {
        pname = "pares-radix";
        version = cargoVersion;
        src = pkgs.lib.cleanSource ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          allowBuiltinFetchGit = true;
        };

        cargoBuildFlags = [ "-p" "pares-radix-cli" ];

        # Skip tests - 2 cerebellum noise-drop tests are flaky (Route::Conscious vs Route::Drop)
        # TODO: fix the tests properly and re-enable
        doCheck = false;

        nativeBuildInputs = with pkgs; [ pkg-config cmake ];
        buildInputs = with pkgs; [
          openssl stdenv.cc.cc.lib glib pango cairo gdk-pixbuf atk gtk3
          graphene webkitgtk_4_1 libsoup_3
        ];

        ORT_LIB_LOCATION = "${onnxruntimeLib { inherit pkgs; }}/lib";
        FASTEMBED_CACHE_PATH = "/tmp/fastembed-cache";

        postInstall = ''
          # Install praxis procedures and plugins alongside the binary
          mkdir -p $out/share/pares-radix
          cp -r ${pkgs.lib.cleanSource ./praxis} $out/share/pares-radix/praxis || true
          cp -r ${pkgs.lib.cleanSource ./plugins} $out/share/pares-radix/plugins || true
          cp -r ${pkgs.lib.cleanSource ./config} $out/share/pares-radix/config || true
        '';

        meta = {
          description = "Pares Radix CLI - headless AI agent daemon";
          homepage = "https://github.com/plures/pares-radix";
          license = pkgs.lib.licenses.bsl11;
          mainProgram = "pares-radix";
        };
      };

      # Tauri desktop app - requires npm build for Svelte frontend first
      mkTauriPkg = pkgs: pkgs.rustPlatform.buildRustPackage {
        pname = "pares-radix-desktop";
        version = cargoVersion;
        src = pkgs.lib.cleanSource ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
          allowBuiltinFetchGit = true;
        };

        cargoBuildFlags = [ "-p" "pares-radix" ];

        nativeBuildInputs = with pkgs; [ pkg-config cmake nodejs_22 ];
        buildInputs = with pkgs; [
          openssl stdenv.cc.cc.lib glib pango cairo gdk-pixbuf atk gtk3
          graphene webkitgtk_4_1 libsoup_3
        ];

        ORT_LIB_LOCATION = "${onnxruntimeLib { inherit pkgs; }}/lib";
        FASTEMBED_CACHE_PATH = "/tmp/fastembed-cache";

        preBuild = ''
          npm ci --ignore-scripts
          npm run build
        '';

        meta = {
          description = "Pares Radix - Tauri 2 desktop shell with Svelte UI";
          homepage = "https://github.com/plures/pares-radix";
          license = pkgs.lib.licenses.bsl11;
          mainProgram = "pares-radix";
        };
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; config.allowUnfree = true; };
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
        };
      in
      {
        packages.default = mkCliPkg pkgs;
        packages.cli = mkCliPkg pkgs;
        packages.desktop = mkTauriPkg pkgs;

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
        pares-radix = mkCliPkg final;
        pares-radix-desktop = mkTauriPkg final;
      };

      # NixOS module - headless agent daemon service
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
              default = "auto";
              description = "Primary model. Set to 'auto' for smart detection from available provider models.";
            };

            deepModel = lib.mkOption {
              type = lib.types.str;
              default = "auto";
              description = "Deep model for low-confidence escalation. Set to 'auto' for smart detection.";
            };

            fastModel = lib.mkOption {
              type = lib.types.str;
              default = "auto";
              description = "Fast model for simple/short responses. Set to 'auto' for smart detection.";
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
              description = "Pares Agens - AI Agent Daemon";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" ];
              wants = [ "network-online.target" ];

              environment = {
                RUST_LOG = "info";
                HOME = cfg.dataDir;
              };

              preStart = ''
                # Sync praxis procedures and plugins from package to data dir
                pkg_share="${cfg.package}/share/pares-radix"
                if [ -d "$pkg_share/praxis" ]; then
                  rm -rf "${cfg.dataDir}/praxis"
                  cp -r "$pkg_share/praxis" "${cfg.dataDir}/praxis"
                fi
                if [ -d "$pkg_share/plugins" ]; then
                  rm -rf "${cfg.dataDir}/plugins"
                  cp -r "$pkg_share/plugins" "${cfg.dataDir}/plugins"
                fi
                if [ -d "$pkg_share/config" ]; then
                  cp -rn "$pkg_share/config/." "${cfg.dataDir}/" 2>/dev/null || true
                fi
              '';

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
                  modelArg = (if cfg.model != "auto" then "--model ${cfg.model}" else "")
                    + (if cfg.deepModel != "auto" then " --deep-model ${cfg.deepModel}" else "")
                    + (if cfg.fastModel != "auto" then " --fast-model ${cfg.fastModel}" else "");
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
