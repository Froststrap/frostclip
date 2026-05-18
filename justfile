APP_WIN := "frostclip.exe"
APP_LINUX := "frostclip-linux"
APP_MAC := "frostclip-macos"

# Windows builds
windows-dev:
    go build -o {{ APP_WIN }} .

windows-release:
    rsrc -ico froststrap.ico -o frostclip.syso
    go build -ldflags="-s -w -H windowsgui" -o {{ APP_WIN }} .
    upx --best {{ APP_WIN }}

# Linux builds
linux-dev:
    GOOS=linux GOARCH=amd64 go build -o {{ APP_LINUX }} .

linux-release:
    GOOS=linux GOARCH=amd64 go build -ldflags="-s -w" -o {{ APP_LINUX }} .
    upx --best {{ APP_LINUX }}

# macOS builds (must run natively on MacOS)
mac-dev:
    go build -o {{ APP_MAC }} .

mac-release-arm64:
    go build -ldflags="-s -w" -o {{ APP_MAC }}-arm64 .

mac-release-amd64:
    CGO_ENABLED=1 GOOS=darwin GOARCH=amd64 CGO_CFLAGS="-arch x86_64" CGO_LDFLAGS="-arch x86_64" go build -ldflags="-s -w" -o {{ APP_MAC }}-amd64 .

mac-release:
    just mac-release-arm64
    just mac-release-amd64

clean:
    rm -f ./frostclip-linux ./frostclip-macos-arm64 ./frostclip-macos-amd64 ./frostclip.exe ./frostclip.syso
