//go:build darwin

package platform

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"frostclip/internal/process"
)

func ScreenResolution() (w, h int, err error) {
	out, e := process.Command("system_profiler", "SPDisplaysDataType").Output()
	if e != nil {
		return 0, 0, e
	}
	re := regexp.MustCompile(`Resolution:\s*(\d+)\s*x\s*(\d+)`)
	m := re.FindSubmatch(out)
	if m == nil {
		return 0, 0, fmt.Errorf("system_profiler: no resolution found")
	}
	w, _ = strconv.Atoi(string(m[1]))
	h, _ = strconv.Atoi(string(m[2]))
	if w == 0 || h == 0 {
		return 0, 0, fmt.Errorf("system_profiler: zero resolution")
	}
	return w, h, nil
}

func RefreshRate() (int, error) {
	out, err := process.Command("system_profiler", "SPDisplaysDataType").Output()
	if err != nil {
		return 0, err
	}
	re := regexp.MustCompile(`(?i)framerate:\s*(\d+)\s*[Hh]z`)
	m := re.FindSubmatch(out)
	if m == nil {
		return 0, fmt.Errorf("system_profiler: no refresh rate found")
	}
	hz, _ := strconv.Atoi(string(m[1]))
	if hz == 0 {
		return 0, fmt.Errorf("system_profiler: zero refresh rate")
	}
	return hz, nil
}

func ClipsDir() string {
	home, err := os.UserHomeDir()
	if err != nil {
		home = os.Getenv("HOME")
	}
	return filepath.Join(home, "Movies", "FrostClips")
}

func runCommand(bin string, args ...string) (string, error) {
	cmd := process.Command(bin, args...)
	var buf strings.Builder
	out, err := cmd.CombinedOutput()
	buf.Write(out)
	return buf.String(), err
}
