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

# macOS builds (must run natively on macOS)
mac-dev:
    go build -o {{ APP_MAC }} .

mac-release:
    GOARCH=amd64 go build -ldflags="-s -w" -o {{ APP_MAC }}-amd64 .
    GOARCH=arm64 go build -ldflags="-s -w" -o {{ APP_MAC }}-arm64 .
    lipo -create -output {{ APP_MAC }} {{ APP_MAC }}-amd64 {{ APP_MAC }}-arm64
    rm {{ APP_MAC }}-amd64 {{ APP_MAC }}-arm64

clean:
    rm -f ./frostclip.exe ./frostclip-linux ./frostclip-macos ./frostclip.syso
