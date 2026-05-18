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

func ScreenResolution() (w, h int, err error) {
	if res, e := resolutionFromWlrRandr(); e == nil {
		return res[0], res[1], nil
	}
	return resolutionFromXrandr()
}

func RefreshRate() (int, error) {
	if hz, e := refreshFromWlrRandr(); e == nil {
		return hz, nil
	}
	return refreshFromXrandr()
}

func ClipsDir() string {
	if xdg := xdgVideosDir(); xdg != "" {
		return filepath.Join(xdg, "FrostClips")
	}
	return filepath.Join(os.Getenv("HOME"), "Videos", "FrostClips")
}

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

var xrandrResRe = regexp.MustCompile(`\s+(\d+)x(\d+)\s+`)
var xrandrHzRe = regexp.MustCompile(`(\d+\.\d+)\*`)

func resolutionFromXrandr() (int, int, error) {
	out, err := process.Command("xrandr", "--current").Output()
	if err != nil {
		return 0, 0, fmt.Errorf("xrandr: %w", err)
	}
	lines := strings.Split(string(out), "\n")
	for _, line := range lines {
		if strings.Contains(line, " connected") {
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

func xdgVideosDir() string {
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
	dir = strings.ReplaceAll(dir, "$HOME", os.Getenv("HOME"))
	return dir
}
