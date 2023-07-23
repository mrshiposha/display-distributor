{ pkgs ? import <nixpkgs> {} }: with pkgs; pkgs.mkShell {
    nativeBuildInputs = [
        pkg-config
    ];
    buildInputs = [
        systemd
        dbus
    ];
}
