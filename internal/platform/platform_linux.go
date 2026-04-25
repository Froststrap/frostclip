//go:build linux

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

// ScreenResolution returns the primary monitor's resolution.
// Tries wlr-randr (Wayland/wlroots) then xrandr (X11/XWayland).
func ScreenResolution() (w, h int, err error) {
	if res, e := resolutionFromWlrRandr(); e == nil {
		return res[0], res[1], nil
	}
	return resolutionFromXrandr()
}

// RefreshRate returns the primary monitor's refresh rate in Hz.
func RefreshRate() (int, error) {
	if hz, e := refreshFromWlrRandr(); e == nil {
		return hz, nil
	}
	return refreshFromXrandr()
}

// ClipsDir returns the default directory for saving clips, honouring XDG.
func ClipsDir() string {
	// Respect XDG_VIDEOS_DIR if set
	if xdg := xdgVideosDir(); xdg != "" {
		return filepath.Join(xdg, "FrostClips")
	}
	return filepath.Join(os.Getenv("HOME"), "Videos", "FrostClips")
}

// --- wlr-randr (Wayland / wlroots compositors: sway, Hyprland, river) ------

var wlrResRe = regexp.MustCompile(`(\d+)x(\d+)\s+px`)
var wlrHzRe = regexp.MustCompile(`([\d.]+)\s+Hz\s+\(preferred\)`)

func resolutionFromWlrRandr() ([2]int, error) {
	out, err := process.Command("wlr-randr").Output()
	if err != nil {
		return [2]int{}, err
	}
	m := wlrResRe.FindSubmatch(out)
	if m == nil {
		return [2]int{}, fmt.Errorf("wlr-randr: no resolution found")
	}
	w, _ := strconv.Atoi(string(m[1]))
	h, _ := strconv.Atoi(string(m[2]))
	if w == 0 || h == 0 {
		return [2]int{}, fmt.Errorf("wlr-randr: zero resolution")
	}
	return [2]int{w, h}, nil
}

func refreshFromWlrRandr() (int, error) {
	out, err := process.Command("wlr-randr").Output()
	if err != nil {
		return 0, err
	}
	m := wlrHzRe.FindSubmatch(out)
	if m == nil {
		return 0, fmt.Errorf("wlr-randr: no preferred refresh rate")
	}
	f, err := strconv.ParseFloat(string(m[1]), 64)
	if err != nil || f == 0 {
		return 0, fmt.Errorf("wlr-randr: bad refresh rate")
	}
	return int(f + 0.5), nil
}

// --- xrandr (X11 / XWayland) -----------------------------------------------

// xrandr output line with resolution: "   1920x1080      60.00*+"
var xrandrResRe = regexp.MustCompile(`\s+(\d+)x(\d+)\s+`)
var xrandrHzRe = regexp.MustCompile(`(\d+\.\d+)\*`)

func resolutionFromXrandr() (int, int, error) {
	out, err := process.Command("xrandr", "--current").Output()
	if err != nil {
		return 0, 0, fmt.Errorf("xrandr: %w", err)
	}
	// Find the first "connected primary" or first "connected" block
	lines := strings.Split(string(out), "\n")
	for _, line := range lines {
		if strings.Contains(line, " connected") {
			// The resolution follows on subsequent lines; keep scanning
			continue
		}
		m := xrandrResRe.FindStringSubmatch(line)
		if m != nil {
			w, _ := strconv.Atoi(m[1])
			h, _ := strconv.Atoi(m[2])
			if w > 0 && h > 0 {
				return w, h, nil
			}
		}
	}
	return 0, 0, fmt.Errorf("xrandr: no resolution found")
}

func refreshFromXrandr() (int, error) {
	out, err := process.Command("xrandr", "--current").Output()
	if err != nil {
		return 0, fmt.Errorf("xrandr: %w", err)
	}
	m := xrandrHzRe.FindSubmatch(out)
	if m == nil {
		return 0, fmt.Errorf("xrandr: no current refresh rate")
	}
	f, err := strconv.ParseFloat(string(m[1]), 64)
	if err != nil || f == 0 {
		return 0, fmt.Errorf("xrandr: bad refresh rate")
	}
	return int(f + 0.5), nil
}

// --- XDG helpers -----------------------------------------------------------

func xdgVideosDir() string {
	// Try parsing ~/.config/user-dirs.dirs
	path := filepath.Join(os.Getenv("HOME"), ".config", "user-dirs.dirs")
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	re := regexp.MustCompile(`XDG_VIDEOS_DIR="([^"]+)"`)
	m := re.FindSubmatch(data)
	if m == nil {
		return ""
	}
	dir := string(m[1])
	// Replace $HOME
	dir = strings.ReplaceAll(dir, "$HOME", os.Getenv("HOME"))
	return dir
}
