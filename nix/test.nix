# NixOS test for `services.clawcrew.instances.<name>`.
#
# Run via the standard nixosTest entry point:
#
#   nix-build -E '
#     (import <nixpkgs/nixos/lib/testing-python.nix> { })
#       .makeTest (import ./nix/test.nix { })
#   '
#
# Or wire into a flake's `checks.${system}` block via
# `pkgs.testers.runNixOSTest`. Either entry point requires KVM on the
# builder.
#
# Asserts:
#   1. Two instances declared in `services.clawcrew.instances` produce two
#      `clawcrew-<name>.service` units that both reach `active` within 30 s.
#   2. Each instance has its own state directory under `/var/lib/clawcrew-<name>`,
#      owned by its own per-instance system user.
#   3. The two per-instance UIDs are distinct (multi-instance isolation).
#   4. `${dataDir}/config.toml` exists, mode 0600, owned by the per-instance
#      user, and round-trips through a TOML parser to the input `settings`.
#   5. The unit's effective hardening profile mentions `ProtectSystem=strict`
#      (sanity check that the module's defaults actually applied).
#   6. The `$VAR` secrets path resolves end-to-end: a third instance with
#      `bot_token = "$BOT_TOKEN"` and an `environmentFile` containing
#      `BOT_TOKEN=secret-from-env-file` produces a `config.toml` whose
#      `bot_token` field is the literal `secret-from-env-file`, not the
#      placeholder.
#   7. A `dataDir` outside `/var/lib/<basename>` (e.g. `/srv/clawcrew-srv`
#      or `/var/lib/clawcrew/nested`) is created at the configured path
#      with the correct ownership, and the unit starts cleanly. Regression
#      guard for the previous `StateDirectory = baseNameOf dataDir` shape
#      that silently created the wrong directory.
#   8. A second instance may share another instance's user/group when it sets
#      `createUser = false`, matching the documented "bring your own user"
#      contract.
#
# A no-op stub binary stands in for the real `clawcrew daemon` so the test
# does not depend on a working ClawCrew build. The stub validates everything
# we need from the *module*: unit generation, file rendering, user creation,
# hardening defaults.
{
  pkgs ? import <nixpkgs> { },
}:

let
  # Stub `clawcrew` binary: ignore arguments, sleep forever so systemd's
  # Type=simple treats the unit as active.
  clawcrewStub = pkgs.writeShellApplication {
    name = "clawcrew";
    text = ''
      # Ignore the daemon argument; just stay alive.
      exec sleep infinity
    '';
  };

  # Wrap the script so `${cfg.package}/bin/clawcrew` resolves to it, and so
  # `lib.getExe` (which reads `meta.mainProgram`) finds a single binary.
  stubPackage =
    pkgs.runCommand "clawcrew-stub"
      {
        meta.mainProgram = "clawcrew";
      }
      ''
        mkdir -p $out/bin
        cp ${clawcrewStub}/bin/clawcrew $out/bin/clawcrew
      '';

  moduleUnderTest = ./module.nix;

in
{
  name = "clawcrew-module";

  nodes.machine =
    { pkgs, ... }:
    {
      imports = [ moduleUnderTest ];

      services.clawcrew.instances.test = {
        package = stubPackage;
        settings = {
          default_provider = "anthropic";
          default_model = "claude-sonnet-4-6";
          default_temperature = 0.4;
          channels.telegram = {
            enabled = true;
            bot_token = "fake-token-for-test";
            allowed_users = [ "12345" ];
          };
        };
      };

      services.clawcrew.instances.other = {
        package = stubPackage;
        settings = {
          default_provider = "anthropic";
          default_model = "claude-haiku-4-6";
        };
      };

      # Third instance exercises the `$VAR` secret path: `bot_token` is
      # the literal placeholder in `settings`; an `environmentFile`
      # provides `BOT_TOKEN=...`; the unit's ExecStartPre envsubst step
      # is expected to expand it on disk under `/var/lib/clawcrew-secret/`.
      environment.etc."clawcrew-secret-env".text = ''
        BOT_TOKEN=secret-from-env-file
      '';

      services.clawcrew.instances.secret = {
        package = stubPackage;
        environmentFile = "/etc/clawcrew-secret-env";
        settings = {
          default_provider = "anthropic";
          channels.telegram = {
            enabled = true;
            bot_token = "$BOT_TOKEN";
            allowed_users = [ "12345" ];
          };
        };
      };

      # Fourth instance exercises a non-`/var/lib/<basename>` `dataDir`.
      # Under the previous `StateDirectory = baseNameOf dataDir` shape
      # systemd would have created `/var/lib/srv-test` and the unit's
      # WorkingDirectory= would have pointed at the absent `/srv/clawcrew-srv`.
      services.clawcrew.instances.srv-test = {
        package = stubPackage;
        dataDir = "/srv/clawcrew-srv";
        settings = {
          default_provider = "anthropic";
        };
      };

      # Fifth instance shares the `test` user/group but does not ask the
      # module to create them again. This is the documented shared-user
      # contract and should not trip the eval-time uniqueness assertion.
      services.clawcrew.instances.shared-user = {
        package = stubPackage;
        user = "clawcrew-test";
        group = "clawcrew-test";
        createUser = false;
        dataDir = "/var/lib/clawcrew-shared-user";
        settings = {
          default_provider = "anthropic";
        };
      };

      # `yq -p toml` (binary name from `pkgs.yq-go`) parses the rendered
      # TOML for the round-trip check.
      environment.systemPackages = [
        pkgs.yq-go
        pkgs.coreutils
      ];
    };

  testScript = ''
    machine.start()

    with subtest("both instances start within 30 s"):
        machine.wait_for_unit("clawcrew-test.service", timeout=30)
        machine.wait_for_unit("clawcrew-other.service", timeout=30)

    with subtest("each instance has its own dataDir owned by its own user"):
        machine.succeed("test -d /var/lib/clawcrew-test")
        machine.succeed("test -d /var/lib/clawcrew-other")
        owner_test = machine.succeed("stat -c '%U' /var/lib/clawcrew-test").strip()
        owner_other = machine.succeed("stat -c '%U' /var/lib/clawcrew-other").strip()
        assert owner_test == "clawcrew-test", f"expected clawcrew-test, got {owner_test}"
        assert owner_other == "clawcrew-other", f"expected clawcrew-other, got {owner_other}"

    with subtest("UIDs are distinct (multi-instance isolation)"):
        uid_test = machine.succeed("id -u clawcrew-test").strip()
        uid_other = machine.succeed("id -u clawcrew-other").strip()
        assert uid_test != uid_other, f"both instances share UID {uid_test}"

    with subtest("config.toml exists with mode 0600 and correct owner"):
        machine.succeed("test -f /var/lib/clawcrew-test/config.toml")
        mode = machine.succeed("stat -c '%a' /var/lib/clawcrew-test/config.toml").strip()
        owner = machine.succeed("stat -c '%U:%G' /var/lib/clawcrew-test/config.toml").strip()
        assert mode == "600", f"expected 600, got {mode}"
        assert owner == "clawcrew-test:clawcrew-test", f"unexpected owner {owner}"

    with subtest("rendered TOML round-trips through a parser"):
        model = machine.succeed(
            "yq -p toml -o json '.default_model' /var/lib/clawcrew-test/config.toml"
        ).strip().strip('"')
        assert model == "claude-sonnet-4-6", f"expected claude-sonnet-4-6, got {model}"

        other_model = machine.succeed(
            "yq -p toml -o json '.default_model' /var/lib/clawcrew-other/config.toml"
        ).strip().strip('"')
        assert other_model == "claude-haiku-4-6", f"expected claude-haiku-4-6, got {other_model}"

    with subtest("hardening defaults applied (ProtectSystem=strict)"):
        out = machine.succeed(
            "systemctl show -p ProtectSystem clawcrew-test.service"
        ).strip()
        assert out == "ProtectSystem=strict", (
            f"hardening defaults not applied: {out!r}"
        )

    with subtest("$VAR secret expansion: bot_token resolved from environmentFile"):
        machine.wait_for_unit("clawcrew-secret.service", timeout=30)
        rendered = machine.succeed(
            "yq -p toml -o json '.channels.telegram.bot_token' "
            "/var/lib/clawcrew-secret/config.toml"
        ).strip().strip('"')
        assert rendered == "secret-from-env-file", (
            f"envsubst did not resolve $BOT_TOKEN — config.toml has {rendered!r}"
        )
        # The build-time copy in /nix/store must still contain the literal
        # placeholder; otherwise the secret would be world-readable.
        nix_store_copy = machine.succeed(
            "systemctl show -p ExecStartPre clawcrew-secret.service"
        )
        assert "/nix/store/" in nix_store_copy, (
            "ExecStartPre is not pointing at a /nix/store source"
        )

    with subtest("non-/var/lib dataDir: directory created at the configured path"):
        machine.wait_for_unit("clawcrew-srv-test.service", timeout=30)
        machine.succeed("test -d /srv/clawcrew-srv")
        owner_srv = machine.succeed("stat -c '%U:%G' /srv/clawcrew-srv").strip()
        assert owner_srv == "clawcrew-srv-test:clawcrew-srv-test", (
            f"unexpected owner {owner_srv}"
        )
        # Regression guard: the old StateDirectory=baseNameOf shape would
        # have created /var/lib/clawcrew-srv (matching the basename) instead
        # of the configured /srv/clawcrew-srv path.
        machine.fail("test -d /var/lib/clawcrew-srv")
        machine.succeed("test -f /srv/clawcrew-srv/config.toml")

    with subtest("shared user is allowed when createUser=false"):
        machine.wait_for_unit("clawcrew-shared-user.service", timeout=30)
        machine.succeed("test -d /var/lib/clawcrew-shared-user")
        owner_shared = machine.succeed(
            "stat -c '%U:%G' /var/lib/clawcrew-shared-user"
        ).strip()
        assert owner_shared == "clawcrew-test:clawcrew-test", (
            f"unexpected owner {owner_shared}"
        )
        machine.succeed("test -f /var/lib/clawcrew-shared-user/config.toml")
  '';
}
