# NixOS module for self-hosting Typstnique, exposed from the flake as
# `nixosModules.default`. Import it and set `services.typstnique.enable = true`.
#
# `self` is the flake itself, used to default the package to the build for the
# host system.
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.typstnique;
in
{
  options.services.typstnique = {
    enable = lib.mkEnableOption "the Typstnique typesetting speed game server";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "typstnique.packages.\${system}.default";
      description = "Typstnique package to run (server binary plus the bundled client site).";
    };

    address = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      example = "0.0.0.0";
      description = ''
        Address the server binds to. Keep the loopback default when running
        behind a reverse proxy; use "0.0.0.0" to listen on all interfaces.
      '';
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 3000;
      description = "TCP port the server listens on.";
    };

    databasePath = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/typstnique/typstnique.db";
      description = ''
        Path to the SQLite leaderboard database; created on first start. The
        default lives under the service's systemd `StateDirectory`, which is
        created and owned automatically. Point it elsewhere only if that path
        is writable by the (dynamic) service user.
      '';
    };

    logLevel = lib.mkOption {
      type = lib.types.str;
      default = "info,hyper=warn,sqlx=warn,tower=warn";
      example = "debug";
      description = "Server `RUST_LOG` / tracing `EnvFilter` value.";
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Optional systemd `EnvironmentFile` for secrets or overrides. It is
        loaded after the variables this module sets, so it can override them.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to open {option}`services.typstnique.port` in the firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.typstnique = {
      isSystemUser = true;
      group = "typstnique";
    };

    users.groups.typstnique = { };

    systemd.services.typstnique = {
      description = "Typstnique typesetting speed game";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      # Leptos reads its runtime configuration from the environment (there is no
      # Cargo.toml at runtime): OUTPUT_NAME plus SITE_ROOT/PKG_DIR locate the
      # bundled client assets, SITE_ADDR is the bind address.
      environment = {
        LEPTOS_OUTPUT_NAME = "typstnique";
        LEPTOS_SITE_ROOT = "${cfg.package}/share/typstnique/site";
        LEPTOS_SITE_PKG_DIR = "pkg";
        LEPTOS_SITE_ADDR = "${cfg.address}:${toString cfg.port}";
        LEPTOS_ENV = "PROD";
        DATABASE_URL = "sqlite:${cfg.databasePath}";
        RUST_LOG = cfg.logLevel;
      };

      serviceConfig = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";

        # Transient, isolated service user with a persistent state directory
        # (/var/lib/typstnique) that holds the SQLite database.
        DynamicUser = false;
        User = "typstnique";
        Group = "typstnique";

        StateDirectory = "typstnique";
        WorkingDirectory = "/var/lib/typstnique";

        EnvironmentFile = lib.optional (cfg.environmentFile != null) cfg.environmentFile;

        # Hardening: the service only needs to listen on a socket and write its
        # database under the state directory.
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
        ];
        RestrictNamespaces = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        RestrictRealtime = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
          "~@resources"
        ];
      };
    };

    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}
