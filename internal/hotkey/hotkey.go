package hotkey

import (
	"runtime"
	"time"
	"unsafe"

	"go.uber.org/zap"
	"golang.org/x/sys/windows"
)

var (
	user32             = windows.NewLazySystemDLL("user32.dll")
	procRegisterHotKey = user32.NewProc("RegisterHotKey")
	procGetMessageW    = user32.NewProc("GetMessageW")
)

const (
	WM_HOTKEY = 0x0312
	VK_F6     = 0x75
	VK_F7     = 0x76
	VK_F8     = 0x77
	VK_F9     = 0x78
)

type MSG struct {
	Hwnd    uintptr
	Message uint32
	WParam  uintptr
	LParam  uintptr
	Time    uint32
	Pt      struct{ X, Y int32 }
}

type SaveRequest struct {
	Seconds int
}

func Listen(saveChan chan<- SaveRequest, log *zap.Logger) {
	log = log.Named("hotkey")
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()

	hotkeys := map[uintptr]struct {
		vk      uintptr
		seconds int
		label   string
	}{
		1: {VK_F6, 10, "F6 (10s)"},
		2: {VK_F7, 15, "F7 (15s)"},
		3: {VK_F8, 30, "F8 (30s)"},
		4: {VK_F9, 60, "F9 (60s)"},
	}

	allRegistered := true
	for id, hk := range hotkeys {
		ret, _, err := procRegisterHotKey.Call(0, id, 0, hk.vk)
		if ret == 0 {
			log.Error("could not register hotkey", zap.String("key", hk.label), zap.Error(err))
			allRegistered = false
		} else {
			log.Info("registered hotkey", zap.String("key", hk.label), zap.Int("seconds", hk.seconds))
		}
	}

	if !allRegistered {
		log.Warn("some hotkeys failed — falling back to polling mode")
		pollingFallback(saveChan, log)
		return
	}

	var msg MSG
	for {
		ret, _, _ := procGetMessageW.Call(
			uintptr(unsafe.Pointer(&msg)),
			0, 0, 0,
		)

		if ret == 0 || ret == ^uintptr(0) {
			break
		}

		if msg.Message == WM_HOTKEY {
			if hk, ok := hotkeys[msg.WParam]; ok {
				log.Info("hotkey pressed", zap.String("key", hk.label), zap.Int("seconds", hk.seconds))
				select {
				case saveChan <- SaveRequest{Seconds: hk.seconds}:
				default:
					log.Warn("save queue full — press again in a moment")
				}
			}
		}
	}
}

func pollingFallback(saveChan chan<- SaveRequest, log *zap.Logger) {
	procGetAsyncKeyState := user32.NewProc("GetAsyncKeyState")

	keys := []struct {
		vk      uintptr
		seconds int
		label   string
		wasDown bool
	}{
		{VK_F6, 10, "F6", false},
		{VK_F7, 15, "F7", false},
		{VK_F8, 30, "F8", false},
		{VK_F9, 60, "F9", false},
	}

	for {
		for i := range keys {
			ret, _, _ := procGetAsyncKeyState.Call(keys[i].vk)
			isDown := ret&0x8000 != 0

			if isDown && !keys[i].wasDown {
				log.Info("hotkey detected", zap.String("key", keys[i].label), zap.Int("seconds", keys[i].seconds))
				select {
				case saveChan <- SaveRequest{Seconds: keys[i].seconds}:
				default:
				}
			}
			keys[i].wasDown = isDown
		}
		time.Sleep(50 * time.Millisecond)
	}
}