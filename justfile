APP := "frostclip"
BUILD_DIR := "build"

prepare:
    mkdir -p {{BUILD_DIR}}

windows-dev: prepare
    cargo build --bin {{APP}}
    cp target/debug/{{APP}}.exe {{BUILD_DIR}}/{{APP}}-windows-dev.exe

linux-dev: prepare
    cargo build --bin {{APP}}
    cp target/debug/{{APP}} {{BUILD_DIR}}/{{APP}}-linux-dev

mac-dev: prepare
    cargo build --bin {{APP}}
    cp target/debug/{{APP}} {{BUILD_DIR}}/{{APP}}-mac-dev

windows-release: prepare
    cargo build --release --bin {{APP}}
    cp target/release/{{APP}}.exe {{BUILD_DIR}}/{{APP}}-windows.exe

linux-release: prepare
    cargo build --release --bin {{APP}}
    cp target/release/{{APP}} {{BUILD_DIR}}/{{APP}}-linux

mac-release: prepare
    cargo build --release --bin {{APP}}
    cp target/release/{{APP}} {{BUILD_DIR}}/{{APP}}-mac

check:
    cargo check

lint:
    cargo clippy -- -D warnings

fmt:
    cargo fmt

clean:
    cargo clean
    rm -rf {{BUILD_DIR}}
