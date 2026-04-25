package tray

import (
	"encoding/binary"
	"runtime"

	"frostclip/internal/hotkey"

	"fyne.io/systray"
	"go.uber.org/zap"
)

func Run(saveChan chan<- hotkey.SaveRequest, log *zap.Logger, iconData []byte) {
	systray.Run(func() { onReady(log, iconData) }, onExit)
}

func Quit() {
	systray.Quit()
}

func onReady(log *zap.Logger, iconData []byte) {
	systray.SetIcon(trayIcon(iconData))
	systray.SetTitle("FrostClip")
	systray.SetTooltip("FrostClip — Clip Recorder")

	m10 := systray.AddMenuItem("Save last 10s", "Save the last 10 seconds")
	m15 := systray.AddMenuItem("Save last 15s", "Save the last 15 seconds")
	m30 := systray.AddMenuItem("Save last 30s", "Save the last 30 seconds")
	m60 := systray.AddMenuItem("Save last 60s", "Save the last 60 seconds")

	systray.AddSeparator()
	mQuit := systray.AddMenuItem("Quit FrostClip", "Exit the application")

	go func() {
		for {
			select {
			case <-m10.ClickedCh:
				log.Info("tray: save 10s clip")
				hotkey.TriggerSave(10)
			case <-m15.ClickedCh:
				log.Info("tray: save 15s clip")
				hotkey.TriggerSave(15)
			case <-m30.ClickedCh:
				log.Info("tray: save 30s clip")
				hotkey.TriggerSave(30)
			case <-m60.ClickedCh:
				log.Info("tray: save 60s clip")
				hotkey.TriggerSave(60)
			case <-mQuit.ClickedCh:
				log.Info("tray: quit requested")
				systray.Quit()
				return
			}
		}
	}()
}

func onExit() {}

func trayIcon(data []byte) []byte {
	if runtime.GOOS == "windows" {
		return data
	}
	if png := extractPNGFromICO(data); png != nil {
		return png
	}
	return data // already PNG, or extraction failed — pass through as-is
}

func extractPNGFromICO(data []byte) []byte {
	const pngMagic = "\x89PNG"

	if len(data) < 6 {
		return nil
	}
	if binary.LittleEndian.Uint16(data[2:]) != 1 { // type must be 1 (ICO)
		return nil
	}
	count := int(binary.LittleEndian.Uint16(data[4:]))

	for i := 0; i < count; i++ {
		entryOff := 6 + i*16
		if entryOff+16 > len(data) {
			break
		}
		imgSize := int(binary.LittleEndian.Uint32(data[entryOff+8:]))
		imgOff := int(binary.LittleEndian.Uint32(data[entryOff+12:]))
		if imgOff+imgSize > len(data) || imgSize < 4 {
			continue
		}
		if string(data[imgOff:imgOff+4]) == pngMagic {
			return data[imgOff : imgOff+imgSize]
		}
	}
	return nil
}
