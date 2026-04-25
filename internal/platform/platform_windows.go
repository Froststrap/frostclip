//go:build windows

package platform

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"frostclip/internal/process"
)

// ScreenResolution returns the primary monitor's resolution in pixels.
func ScreenResolution() (w, h int, err error) {
	cmd := process.Command("powershell", "-NoProfile", "-Command",
		`$s = Get-CimInstance -ClassName Win32_VideoController | Select-Object -First 1; "$($s.CurrentHorizontalResolution)x$($s.CurrentVerticalResolution)"`,
	)
	out, err := cmd.Output()
	if err != nil {
		return 0, 0, err
	}
	parts := strings.TrimSpace(string(out))
	if _, err := fmt.Sscanf(parts, "%dx%d", &w, &h); err != nil {
		return 0, 0, fmt.Errorf("could not parse resolution %q", parts)
	}
	if w == 0 || h == 0 {
		return 0, 0, fmt.Errorf("got zero resolution")
	}
	return w, h, nil
}

// RefreshRate returns the primary monitor's refresh rate in Hz.
func RefreshRate() (int, error) {
	cmd := process.Command("powershell", "-NoProfile", "-Command",
		`(Get-CimInstance -ClassName Win32_VideoController | Select-Object -First 1).CurrentRefreshRate`,
	)
	out, err := cmd.Output()
	if err != nil {
		return 0, err
	}
	var hz int
	if _, err := fmt.Sscanf(strings.TrimSpace(string(out)), "%d", &hz); err != nil {
		return 0, fmt.Errorf("could not parse refresh rate %q", strings.TrimSpace(string(out)))
	}
	if hz == 0 {
		return 0, fmt.Errorf("got zero refresh rate")
	}
	return hz, nil
}

// ClipsDir returns the default directory for saving clips.
func ClipsDir() string {
	return filepath.Join(os.Getenv("USERPROFILE"), "Videos", "FrostClips")
}
