{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  # https://devenv.sh/packages/
  packages = with pkgs; [
    git
    probe-rs-tools
    nixfmt-rfc-style
  ];

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    channel = "nightly";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src"
    ];
    targets = [ "thumbv7em-none-eabi" ];
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy = {
      enable = true;
      settings.allFeatures = true;
    };

    unit-tests = {
      enable = true;
      name = "nix linter -c";
      entry = "nixfmt";
      files = "\\.nix$";
    };
  };
}
