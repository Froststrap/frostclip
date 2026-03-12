set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

APP := "frostclip.exe"

dev:
    go build -o {{APP}} .

release:
    rsrc -ico froststrap.ico -o frostclip.syso
    go build -ldflags="-s -w -H windowsgui" -o {{APP}} .
    upx --best {{APP}}

clean:
    Stop-Process -Name "frostclip" -Force
    Start-Sleep -Seconds 2
    Remove-Item -Force ./frostclip.exe
