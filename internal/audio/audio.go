package audio

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sync"
)

//go:embed silence_windows.wav
var silenceWindows []byte

//go:embed silence_darwin.wav
var silenceDarwin []byte

//go:embed silence_linux.wav
var silenceLinux []byte

var (
	ensureOnce sync.Once
	ensurePath string
	ensureErr  error
)

func EnsureSilenceFile() (string, error) {
	ensureOnce.Do(func() {
		data := silenceData()
		if len(data) == 0 {
			ensureErr = fmt.Errorf("no bundled silence audio for %s", runtime.GOOS)
			return
		}
		dir := filepath.Join(os.TempDir(), "frostclip")
		if err := os.MkdirAll(dir, 0755); err != nil {
			ensureErr = err
			return
		}
		path := filepath.Join(dir, fmt.Sprintf("silence_%s.wav", runtime.GOOS))
		if info, err := os.Stat(path); err == nil && info.Size() > 0 {
			ensurePath = path
			return
		}
		if err := os.WriteFile(path, data, 0644); err != nil {
			ensureErr = err
			return
		}
		ensurePath = path
	})
	return ensurePath, ensureErr
}

func silenceData() []byte {
	switch runtime.GOOS {
	case "windows":
		return silenceWindows
	case "darwin":
		return silenceDarwin
	case "linux":
		return silenceLinux
	default:
		return nil
	}
}
