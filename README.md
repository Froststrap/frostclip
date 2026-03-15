Game clipper for [Froststrap](https://github.com/Froststrap/Froststrap)

## CURRENTLY IN ALPHA STAGE

However, it is usable (for AMD and Intel not too sure aobut NVIDIA), if you are on NVIDIA and it doesn't work head over to the [Discord server](https://discord.gg/9nvJVuaqy4)

### Dependencies

- [Go](https://go.dev/dl/)
- [just](https://github.com/casey/just) (optional but recommended)
- [rsrc](https://github.com/akavel/rsrc/) (used to add the icon to the executable, optional if you don't care about that)
- [UPX](https://upx.github.io/) (optional for smaller release builds)

### How to build

1. clone the repo
2. run `go mod tidy` to install dependencies
3. run `just dev` to build the project with a console window
4. run `just release` to build the project for production use

### Manuual build

1. clone the repo
2. run `go mod tidy` to install dependencies
3. [Optional] run `rsrc -ico froststrap.ico -o frostclip.syso` to add the icon to the executable (requires rsrc to be installed)
4. run `go build -o frostclip.exe .` for development build (with console window)
5. run `go build -ldflags="-s -w -H windowsgui" -o frostclip.exe .` for production build (no console window)
6. [Optional] run `upx --best frostclip.exe` to compress the executable (requires UPX to be installed)
