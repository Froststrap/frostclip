package hotkey

import (
	"bufio"
	"os"
	"strings"

	"go.uber.org/zap"
)

type SaveRequest struct {
	Seconds int
}

// Global reference so the tray can access the main save channel
var internalSaveChan chan<- SaveRequest

// Listen starts the CLI-based simulation to avoid X11 compilation errors.
func Listen(saveChan chan<- SaveRequest, log *zap.Logger) {
	internalSaveChan = saveChan
	log.Info("Hotkey listener active (CLI Simulation Mode)")

	// Temporary until I find hotkey method that works on Wayland
	bindings := map[string]int{
		"f6": 10,
		"f7": 15,
		"f8": 30,
		"f9": 60,
	}

	go func() {
		scanner := bufio.NewScanner(os.Stdin)
		for scanner.Scan() {
			input := strings.ToLower(strings.TrimSpace(scanner.Text()))
			if seconds, ok := bindings[input]; ok {
				log.Info("CLI simulated hotkey pressed", zap.Int("seconds", seconds))
				TriggerSave(seconds)
			}
		}
	}()
}

func TriggerSave(seconds int) {
	if internalSaveChan != nil {
		go func() {
			internalSaveChan <- SaveRequest{Seconds: seconds}
		}()
	}
}
