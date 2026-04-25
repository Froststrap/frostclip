package tray

import (
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
	systray.SetIcon(iconData)
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
