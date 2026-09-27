name := 'cosmic-tailscale'
appid := 'io.github.chispes.CosmicTailscale'
rootdir := ''
prefix := env('HOME') / '.local'
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
base-dir := rootdir + prefix

build-release:
    cargo build --release --locked

run:
    cargo run --release --locked

check:
    cargo clippy --all-features --locked -- -D warnings

install:
    install -Dm0755 {{cargo-target-dir}}/release/{{name}} {{base-dir}}/bin/{{name}}
    install -Dm0644 target/xdgen/app.desktop {{base-dir}}/share/applications/{{appid}}.desktop
    install -Dm0644 target/xdgen/app.metainfo.xml {{base-dir}}/share/metainfo/{{appid}}.metainfo.xml
    install -Dm0644 resources/icon.svg {{base-dir}}/share/icons/hicolor/scalable/apps/{{appid}}.svg

uninstall:
    rm -f {{base-dir}}/bin/{{name}} {{base-dir}}/share/applications/{{appid}}.desktop {{base-dir}}/share/metainfo/{{appid}}.metainfo.xml {{base-dir}}/share/icons/hicolor/scalable/apps/{{appid}}.svg

vendor:
    mkdir -p .cargo
    rm -rf vendor/xml-crate
    cargo vendor --locked > .cargo/config.toml
    mv vendor/xml vendor/xml-crate

dist: vendor
    mkdir -p /tmp/cosmic-tailscale-dist
    tar --transform='s,^,cosmic-tailscale-0.1.0/,' -czf /tmp/cosmic-tailscale-dist/cosmic-tailscale-0.1.0.tar.gz Cargo.toml Cargo.lock build.rs i18n.toml i18n src resources packaging justfile LICENSE .cargo vendor
