package tray

import (
	"frostclip/internal/hotkey"

	"fyne.io/systray"
	"go.uber.org/zap"
)

func Run(saveChan chan<- hotkey.SaveRequest, log *zap.Logger, iconData []byte) {
	systray.Run(func() { onReady(saveChan, log, iconData) }, onExit)
}

func Quit() {
	systray.Quit()
}

func onReady(saveChan chan<- hotkey.SaveRequest, log *zap.Logger, iconData []byte) {
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
				send(saveChan, 10)
			case <-m15.ClickedCh:
				log.Info("tray: save 15s clip")
				send(saveChan, 15)
			case <-m30.ClickedCh:
				log.Info("tray: save 30s clip")
				send(saveChan, 30)
			case <-m60.ClickedCh:
				log.Info("tray: save 60s clip")
				send(saveChan, 60)
			case <-mQuit.ClickedCh:
				log.Info("tray: quit requested")
				systray.Quit()
				return
			}
		}
	}()
}

func onExit() {}

func send(saveChan chan<- hotkey.SaveRequest, seconds int) {
	select {
	case saveChan <- hotkey.SaveRequest{Seconds: seconds}:
	default:
	}
}
