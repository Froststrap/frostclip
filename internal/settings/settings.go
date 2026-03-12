package settings

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"

	"frostclip/internal/process"
	"go.uber.org/zap"
)

func bitrateForResolutionAndFPS(resolution string, fps int) string {
	var w, h int
	if _, err := fmt.Sscanf(resolution, "%dx%d", &w, &h); err != nil || w == 0 || h == 0 {
		return "6000k"
	}
	if fps <= 0 {
		fps = 60
	}
	const (
		basePixels = 1920 * 1080
		baseFPS    = 60
		baseKbps   = 6000
	)
	pixels := w * h
	fpsScale := math.Sqrt(float64(fps) / float64(baseFPS))
	kbps := int(float64(baseKbps) * float64(pixels) / float64(basePixels) * fpsScale)
	kbps = ((kbps + 250) / 500) * 500
	kbps = max(kbps, 2000)
	return fmt.Sprintf("%dk", kbps)
}

type AudioMode string

const (
	AudioMicrophone AudioMode = "microphone"
	AudioSystem     AudioMode = "system"
	AudioBoth       AudioMode = "both"
	AudioOff        AudioMode = "off"
)

type Settings struct {
	FPSRaw     any    `json:"fps"`
	Resolution string `json:"resolution"`
	Bitrate    string `json:"bitrate"`
	Audio      string `json:"audio"`

	FPS       int       `json:"-"`
	AudioMode AudioMode `json:"-"`
}

var defaults = Settings{
	FPSRaw:     "refresh_rate",
	Resolution: "full_screen",
	Bitrate:    "auto",
	Audio:      "microphone",
}

func Load(log *zap.Logger) (*Settings, error) {
	log = log.Named("settings")

	settingsPath, err := settingsFilePath()
	if err != nil {
		return nil, err
	}

	if _, err := os.Stat(settingsPath); os.IsNotExist(err) {
		log.Info("no settings.json found — creating with defaults", zap.String("path", settingsPath))
		if err := writeDefaults(settingsPath); err != nil {
			return nil, fmt.Errorf("failed to create settings.json: %w", err)
		}
	}

	data, err := os.ReadFile(settingsPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read settings.json: %w", err)
	}

	s := defaults
	if err := json.Unmarshal(data, &s); err != nil {
		return nil, fmt.Errorf("invalid settings.json: %w", err)
	}

	if strings.ToLower(s.Resolution) == "full_screen" {
		w, h, err := getScreenResolution()
		if err != nil {
			log.Warn("could not detect screen resolution — defaulting to 1920x1080", zap.Error(err))
			w, h = 1920, 1080
		}
		s.Resolution = fmt.Sprintf("%dx%d", w, h)
		log.Info("resolution", zap.String("value", s.Resolution))
	}

	switch v := s.FPSRaw.(type) {
	case string:
		if strings.ToLower(v) == "refresh_rate" {
			hz, err := getRefreshRate()
			if err != nil {
				log.Warn("could not detect refresh rate — defaulting to 60fps", zap.Error(err))
				hz = 60
			}
			s.FPS = hz
			log.Info("auto fps from display", zap.Int("fps", s.FPS))
		} else {
			log.Warn("unrecognised fps value — defaulting to 60", zap.String("value", v))
			s.FPS = 60
		}
	case float64:
		s.FPS = int(v)
	default:
		log.Warn("invalid fps in settings — defaulting to 60")
		s.FPS = 60
	}

	if s.Bitrate == "" || strings.ToLower(s.Bitrate) == "auto" {
		s.Bitrate = bitrateForResolutionAndFPS(s.Resolution, s.FPS)
		log.Info("auto bitrate", zap.String("bitrate", s.Bitrate), zap.Int("fps", s.FPS), zap.String("resolution", s.Resolution))
	}

	switch AudioMode(strings.ToLower(s.Audio)) {
	case AudioSystem, AudioBoth, AudioOff:
		s.AudioMode = AudioMode(strings.ToLower(s.Audio))
	default:
		s.AudioMode = AudioMicrophone
	}

	log.Info("loaded settings",
		zap.Int("fps", s.FPS),
		zap.String("resolution", s.Resolution),
		zap.String("bitrate", s.Bitrate),
		zap.String("audio", string(s.AudioMode)),
	)
	_ = s.FPSRaw
	return &s, nil
}

func writeDefaults(path string) error {
	content := "{\n  \"fps\": \"refresh_rate\",\n  \"resolution\": \"full_screen\",\n  \"bitrate\": \"auto\",\n  \"audio\": \"microphone\"\n}\n"
	return os.WriteFile(path, []byte(content), 0644)
}

func getScreenResolution() (int, int, error) {
	cmd := process.Command("powershell", "-NoProfile", "-Command",
		`$s = Get-CimInstance -ClassName Win32_VideoController | Select-Object -First 1; "$($s.CurrentHorizontalResolution)x$($s.CurrentVerticalResolution)"`,
	)
	out, err := cmd.Output()
	if err != nil {
		return 0, 0, err
	}

	parts := strings.TrimSpace(string(out))
	var w, h int
	if _, err := fmt.Sscanf(parts, "%dx%d", &w, &h); err != nil {
		return 0, 0, fmt.Errorf("could not parse resolution %q", parts)
	}
	if w == 0 || h == 0 {
		return 0, 0, fmt.Errorf("got zero resolution")
	}
	return w, h, nil
}

func getRefreshRate() (int, error) {
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

func settingsFilePath() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	return filepath.Join(filepath.Dir(exe), "settings.json"), nil
}
