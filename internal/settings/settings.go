package settings

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"

	"frostclip/internal/notify"
	"frostclip/internal/platform"

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

// VolumeMixer controls per-application volume levels by process name or ID
type VolumeMixer map[string]float64 // key: process name or PID (string), value: volume 0-100

type Settings struct {
	FPSRaw         any         `json:"fps"`
	Resolution     string      `json:"resolution"`
	Bitrate        string      `json:"bitrate"`
	Audio          string      `json:"audio"`
	SegmentTempDir string      `json:"segment_temp_dir"`
	AutoUpload     bool        `json:"auto_upload"` // auto-upload clips to FrostClip and copy link
	VolumeMixer    VolumeMixer `json:"volume_mixer"` // per-app volume control

	FPS       int       `json:"-"`
	AudioMode AudioMode `json:"-"`
}

var defaults = Settings{
	FPSRaw:         "refresh_rate",
	Resolution:     "full_screen",
	Bitrate:        "auto",
	Audio:          "system",
	SegmentTempDir: "",
	AutoUpload:     false,
	VolumeMixer:    VolumeMixer{},
}

func Load(log *zap.Logger) (*Settings, error) {
	log = log.Named("settings")
	settingsPath, err := settingsFilePath()
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(settingsPath); os.IsNotExist(err) {
		log.Info("no settings.json — creating defaults", zap.String("path", settingsPath))
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
		w, h, err := platform.ScreenResolution()
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
			hz, err := platform.RefreshRate()
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
		log.Info("auto bitrate", zap.String("bitrate", s.Bitrate))
	}
	switch AudioMode(strings.ToLower(s.Audio)) {
	case AudioSystem, AudioBoth, AudioOff:
		s.AudioMode = AudioMode(strings.ToLower(s.Audio))
	default:
		s.AudioMode = AudioMicrophone
	}
	s.SegmentTempDir = strings.TrimSpace(s.SegmentTempDir)
	log.Info("loaded settings",
		zap.Int("fps", s.FPS),
		zap.String("resolution", s.Resolution),
		zap.String("bitrate", s.Bitrate),
		zap.String("audio", string(s.AudioMode)),
		zap.String("segment_temp_dir", s.SegmentTempDir),
		zap.Bool("auto_upload", s.AutoUpload),
	)
	return &s, nil
}

func writeDefaults(path string) error {
	content := "{\n  \"fps\": \"refresh_rate\",\n  \"resolution\": \"full_screen\",\n  \"bitrate\": \"auto\",\n  \"audio\": \"system\",\n  \"segment_temp_dir\": \"\",\n  \"auto_upload\": false,\n  \"volume_mixer\": {\n    \"discord\": 100,\n    \"firefox\": 100\n  }\n}\n"
	return os.WriteFile(path, []byte(content), 0644)
}

func settingsFilePath() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	return filepath.Join(filepath.Dir(exe), "settings.json"), nil
}

// SettingsUpdate represents a settings change notification
type SettingsUpdate struct {
	Settings *Settings
	Changed  map[string]bool // which fields changed
}

// Watcher monitors settings.json for changes and broadcasts updates
type Watcher struct {
	log      *zap.Logger
	updateCh chan SettingsUpdate
	stopCh   chan struct{}
	mu       sync.Mutex
	last     *Settings
	watcher  *fsnotify.Watcher
	done     chan struct{}
}

// NewWatcher creates a new settings file watcher
func NewWatcher(log *zap.Logger) (*Watcher, error) {
	log = log.Named("settings.watcher")
	fw, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}

	settingsPath, err := settingsFilePath()
	if err != nil {
		fw.Close()
		return nil, err
	}

	// Watch the directory, not the file (file replacement on save creates a new inode)
	if err := fw.Add(filepath.Dir(settingsPath)); err != nil {
		fw.Close()
		return nil, err
	}

	return &Watcher{
		log:      log,
		updateCh: make(chan SettingsUpdate, 1),
		stopCh:   make(chan struct{}),
		watcher:  fw,
		done:     make(chan struct{}),
	}, nil
}

// Watch starts watching for settings changes and returns a channel for updates
// It handles debouncing and error recovery automatically
func (w *Watcher) Watch() <-chan SettingsUpdate {
	go w.run()
	return w.updateCh
}

func (w *Watcher) run() {
	defer close(w.done)
	defer w.watcher.Close()

	debounceTimer := time.NewTimer(100 * time.Millisecond)
	debounceTimer.Stop()
	var debounceActive bool

	for {
		select {
		case <-w.stopCh:
			return
		case event, ok := <-w.watcher.Events:
			if !ok {
				return
			}
			// Only care about writes to settings.json
			if !strings.HasSuffix(event.Name, "settings.json") {
				continue
			}
			if event.Op&(fsnotify.Write|fsnotify.Create) == 0 {
				continue
			}
			// Start/restart debounce timer
			if debounceActive {
				debounceTimer.Stop()
			}
			debounceTimer.Reset(100 * time.Millisecond)
			debounceActive = true

		case <-debounceTimer.C:
			debounceActive = false
			// Load and validate new settings
			if updated, changes := w.loadAndNotify(); updated {
				w.log.Info("settings updated", zap.Any("changes", changes))
			}

		case err, ok := <-w.watcher.Errors:
			if !ok {
				return
			}
			w.log.Warn("watcher error", zap.Error(err))
		}
	}
}

func (w *Watcher) loadAndNotify() (bool, map[string]bool) {
	settingsPath, err := settingsFilePath()
	if err != nil {
		w.log.Warn("could not get settings path", zap.Error(err))
		return false, nil
	}

	data, err := os.ReadFile(settingsPath)
	if err != nil {
		w.log.Warn("could not read settings.json", zap.Error(err))
		return false, nil
	}

	s := defaults
	if err := json.Unmarshal(data, &s); err != nil {
		w.log.Warn("invalid settings.json", zap.Error(err))
		return false, nil
	}

	// Process resolution and fps like the Load function
	if strings.ToLower(s.Resolution) == "full_screen" {
		width, height, err := platform.ScreenResolution()
		if err != nil {
			w.log.Debug("could not detect screen resolution", zap.Error(err))
			width, height = 1920, 1080
		}
		s.Resolution = fmt.Sprintf("%dx%d", width, height)
	}

	// Process FPS
	switch v := s.FPSRaw.(type) {
	case string:
		if strings.ToLower(v) == "refresh_rate" {
			hz, err := platform.RefreshRate()
			if err != nil {
				w.log.Debug("could not detect refresh rate", zap.Error(err))
				hz = 60
			}
			s.FPS = hz
		} else {
			s.FPS = 60
		}
	case float64:
		s.FPS = int(v)
	default:
		s.FPS = 60
	}

	// Process bitrate
	if s.Bitrate == "" || strings.ToLower(s.Bitrate) == "auto" {
		s.Bitrate = bitrateForResolutionAndFPS(s.Resolution, s.FPS)
	}

	// Process audio mode
	switch AudioMode(strings.ToLower(s.Audio)) {
	case AudioSystem, AudioBoth, AudioOff:
		s.AudioMode = AudioMode(strings.ToLower(s.Audio))
	default:
		s.AudioMode = AudioMicrophone
	}

	s.SegmentTempDir = strings.TrimSpace(s.SegmentTempDir)

	// Determine what changed
	w.mu.Lock()
	defer w.mu.Unlock()

	changes := make(map[string]bool)
	if w.last != nil {
		if w.last.FPS != s.FPS {
			changes["fps"] = true
		}
		if w.last.Resolution != s.Resolution {
			changes["resolution"] = true
		}
		if w.last.Bitrate != s.Bitrate {
			changes["bitrate"] = true
		}
		if w.last.AudioMode != s.AudioMode {
			changes["audio"] = true
		}
		if w.last.SegmentTempDir != s.SegmentTempDir {
			changes["segment_temp_dir"] = true
		}
		if w.last.AutoUpload != s.AutoUpload {
			changes["auto_upload"] = true
		}

		// If nothing changed, don't notify
		if len(changes) == 0 {
			return false, nil
		}
	} else {
		// First load, mark all as changed
		changes["fps"] = true
		changes["resolution"] = true
		changes["bitrate"] = true
		changes["audio"] = true
		changes["segment_temp_dir"] = true
		changes["auto_upload"] = true
	}

	w.last = &s

	// Build notification message
	notifyMsg := w.buildNotificationMessage(changes, &s)

	// Send update through channel (non-blocking)
	select {
	case w.updateCh <- SettingsUpdate{Settings: &s, Changed: changes}:
	default:
		// Channel full, skip update (shouldn't happen with buffer size 1)
	}

	// Send desktop notification (non-blocking, in background)
	if notifyMsg != "" && len(changes) > 0 && w.last != nil {
		go func() {
			notify.Send("FrostClip Settings Changed", notifyMsg)
		}()
	}

	return true, changes
}

// buildNotificationMessage creates a user-friendly notification message
func (w *Watcher) buildNotificationMessage(changes map[string]bool, s *Settings) string {
	var parts []string

	if changes["fps"] {
		parts = append(parts, fmt.Sprintf("FPS: %d", s.FPS))
	}
	if changes["resolution"] {
		parts = append(parts, fmt.Sprintf("Resolution: %s", s.Resolution))
	}
	if changes["bitrate"] {
		parts = append(parts, fmt.Sprintf("Bitrate: %s", s.Bitrate))
	}
	if changes["audio"] {
		parts = append(parts, fmt.Sprintf("Audio: %s", s.AudioMode))
	}
	if changes["auto_upload"] {
		if s.AutoUpload {
			parts = append(parts, "Auto-upload: ON")
		} else {
			parts = append(parts, "Auto-upload: OFF")
		}
	}
	if changes["segment_temp_dir"] {
		if s.SegmentTempDir != "" {
			parts = append(parts, fmt.Sprintf("Temp dir: %s", s.SegmentTempDir))
		} else {
			parts = append(parts, "Temp dir: default")
		}
	}
	if changes["volume_mixer"] {
		if len(s.VolumeMixer) > 0 {
			parts = append(parts, fmt.Sprintf("Volume mixer: %d app(s)", len(s.VolumeMixer)))
		} else {
			parts = append(parts, "Volume mixer: disabled")
		}
	}

	if len(parts) == 0 {
		return ""
	}

	return strings.Join(parts, "\n")
}

// Stop stops the watcher
func (w *Watcher) Stop() {
	close(w.stopCh)
	<-w.done
}

