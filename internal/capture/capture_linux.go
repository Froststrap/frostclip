//go:build linux

package capture

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"time"

	"frostclip/internal/buffer"
	"frostclip/internal/platform"
	"frostclip/internal/settings"

	"github.com/fsnotify/fsnotify"
	"go.uber.org/zap"
)

// RecorderManager manages the gpu-screen-recorder process.
type RecorderManager struct {
	mu        sync.Mutex
	cmd       *exec.Cmd
	outputDir string
	cfg       Config
	log       *zap.Logger
	cancel    context.CancelFunc
	watcher   *fsnotify.Watcher
	fileChan  chan string
	restartCh chan struct{}
	stopCh    chan struct{}
	running   bool
	lastSaved time.Time
}

// NewRecorderManager creates a new manager.
func NewRecorderManager(cfg Config, log *zap.Logger) (*RecorderManager, error) {
	// Check if gpu-screen-recorder is installed
	if _, err := exec.LookPath("gpu-screen-recorder"); err != nil {
		return nil, fmt.Errorf("gpu-screen-recorder not found: %w", err)
	}

	outputDir := platform.ClipsDir()
	if err := os.MkdirAll(outputDir, 0755); err != nil {
		return nil, fmt.Errorf("create output dir: %w", err)
	}

	watcher, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, fmt.Errorf("create watcher: %w", err)
	}
	if err := watcher.Add(outputDir); err != nil {
		watcher.Close()
		return nil, fmt.Errorf("watch output dir: %w", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	rm := &RecorderManager{
		outputDir: outputDir,
		cfg:       cfg,
		log:       log.Named("recorder"),
		cancel:    cancel,
		watcher:   watcher,
		fileChan:  make(chan string, 10),
		restartCh: make(chan struct{}, 1),
		stopCh:    make(chan struct{}),
	}

	// Start file watcher
	go rm.fileWatcher(ctx)

	// Start recorder
	if err := rm.startRecorder(); err != nil {
		watcher.Close()
		cancel()
		return nil, err
	}

	go rm.monitorLoop()

	return rm, nil
}

// startRecorder launches gpu-screen-recorder with replay buffer.
func (rm *RecorderManager) startRecorder() error {
	rm.mu.Lock()
	defer rm.mu.Unlock()

	if rm.cmd != nil {
		_ = rm.cmd.Process.Kill()
		_ = rm.cmd.Wait()
	}

	args := rm.buildArgs()
	rm.log.Info("starting gpu-screen-recorder", zap.Strings("args", args))
	cmd := exec.Command("gpu-screen-recorder", args...)
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start recorder: %w", err)
	}
	rm.cmd = cmd
	rm.running = true
	rm.log.Info("recorder started", zap.Int("pid", cmd.Process.Pid))
	return nil
}

// buildArgs constructs the command line arguments.
func (rm *RecorderManager) buildArgs() []string {
	args := []string{
		"-w", "screen",
		"-f", fmt.Sprintf("%d", rm.cfg.Framerate),
		"-r", "60",
		"-c", "vaapi", // Intel/AMD hardware encoding (NVENC on NVIDIA)
		"-o", rm.outputDir,
	}

	switch rm.cfg.AudioMode {
	case settings.AudioSystem:
		if dev := detectLoopbackDevice(rm.log); dev != "" {
			args = append(args, "-a", dev)
		}
	case settings.AudioMicrophone:
		if dev := detectMicDevice(rm.log); dev != "" {
			args = append(args, "-a", dev)
		}
	case settings.AudioBoth:
		if loop := detectLoopbackDevice(rm.log); loop != "" {
			args = append(args, "-a", loop)
		}
		if mic := detectMicDevice(rm.log); mic != "" {
			args = append(args, "-a", mic)
		}
	case settings.AudioOff:
		// no -a flags
	}

	// Optional: codec and quality
	// Use NVENC/VA-API automatically, or set via env? gpu-screen-recorder picks best.
	// We can also set -c to force codec, but let's leave default.

	return args
}

// getAudioDevice returns the PulseAudio/PipeWire device string.
func (rm *RecorderManager) getAudioDevice() string {
	// Use the same detection logic as before
	mode := rm.cfg.AudioMode // you need to store AudioMode in Config
	switch mode {
	case settings.AudioOff:
		return ""
	case settings.AudioSystem:
		return detectLoopbackDevice(rm.cfg.FFmpegBin, rm.log) // returns "default.monitor" etc.
	case settings.AudioBoth:
		loopback := detectLoopbackDevice(rm.cfg.FFmpegBin, rm.log)
		mic := detectMicDevice(rm.cfg.FFmpegBin, rm.log)
		if loopback != "" && mic != "" {
			// gpu-screen-recorder can take multiple -a flags: -a loopback -a mic
			// We'll just return a comma-separated? Actually it expects one device.
			// For simplicity, we'll use the loopback and ignore mic for now.
			return loopback
		}
		return loopback
	case settings.AudioMicrophone:
		return detectMicDevice(rm.cfg.FFmpegBin, rm.log)
	default:
		return ""
	}
}

// SaveClip triggers a clip save and returns the file path.
func (rm *RecorderManager) SaveClip() (string, error) {
	rm.mu.Lock()
	if !rm.running || rm.cmd == nil {
		rm.mu.Unlock()
		return "", fmt.Errorf("recorder not running")
	}
	pid := rm.cmd.Process.Pid
	rm.mu.Unlock()

	// Send SIGUSR1 to the recorder process
	if err := syscall.Kill(pid, syscall.SIGUSR1); err != nil {
		return "", fmt.Errorf("send SIGUSR1: %w", err)
	}
	rm.log.Debug("sent SIGUSR1 to recorder")

	// Wait for a new file
	timeout := time.After(5 * time.Second)
	for {
		select {
		case path := <-rm.fileChan:
			// Verify file exists and is stable
			if _, err := os.Stat(path); err == nil {
				rm.log.Info("clip saved", zap.String("path", path))
				return path, nil
			}
			// If file doesn't exist, continue waiting
		case <-timeout:
			return "", fmt.Errorf("timeout waiting for clip file")
		}
	}
}

// fileWatcher monitors the output directory for new files.
func (rm *RecorderManager) fileWatcher(ctx context.Context) {
	defer rm.watcher.Close()
	for {
		select {
		case <-ctx.Done():
			return
		case event, ok := <-rm.watcher.Events:
			if !ok {
				return
			}
			// Look for newly created files with .mp4 or .mkv
			if event.Op&fsnotify.Create == fsnotify.Create {
				if strings.HasSuffix(event.Name, ".mp4") || strings.HasSuffix(event.Name, ".mkv") {
					// Give the file a moment to be fully written
					time.Sleep(200 * time.Millisecond)
					select {
					case rm.fileChan <- event.Name:
					default:
						rm.log.Warn("fileChan full, dropping file", zap.String("file", event.Name))
					}
				}
			}
		case err, ok := <-rm.watcher.Errors:
			if !ok {
				return
			}
			rm.log.Warn("watcher error", zap.Error(err))
		}
	}
}

// monitorLoop watches for settings changes and restarts if needed.
func (rm *RecorderManager) monitorLoop() {
	for {
		select {
		case <-rm.restartCh:
			rm.log.Info("restarting recorder due to settings change")
			if err := rm.startRecorder(); err != nil {
				rm.log.Error("failed to restart recorder", zap.Error(err))
			}
		case <-rm.stopCh:
			return
		}
	}
}

// UpdateConfig applies new settings and restarts if necessary.
func (rm *RecorderManager) UpdateConfig(cfg Config) {
	rm.mu.Lock()
	oldFps := rm.cfg.Framerate
	oldAudio := rm.cfg.AudioMode
	rm.cfg = cfg
	rm.mu.Unlock()

	// Check if relevant settings changed
	if cfg.Framerate != oldFps || cfg.AudioMode != oldAudio {
		select {
		case rm.restartCh <- struct{}{}:
		default:
		}
	}
}

// Stop kills the recorder and cleans up.
func (rm *RecorderManager) Stop() error {
	rm.mu.Lock()
	defer rm.mu.Unlock()
	if rm.cmd != nil {
		if err := rm.cmd.Process.Kill(); err != nil {
			rm.log.Warn("kill recorder", zap.Error(err))
		}
		_ = rm.cmd.Wait()
		rm.running = false
	}
	rm.cancel()
	close(rm.stopCh)
	return nil
}

// Global recorder instance
var globalRecorder *RecorderManager

// Loop starts the recorder and blocks until stop.
func Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	var err error
	globalRecorder, err = NewRecorderManager(cfg, log)
	if err != nil {
		log.Fatal("failed to initialize recorder", zap.Error(err))
	}
	defer globalRecorder.Stop()

	// Monitor for settings updates
	if cfg.UpdateCh != nil {
		go func() {
			for update := range cfg.UpdateCh {
				if update.Changed["fps"] || update.Changed["audio"] {
					newCfg := cfg
					newCfg.Framerate = update.Settings.FPS
					newCfg.AudioMode = update.Settings.AudioMode
					globalRecorder.UpdateConfig(newCfg)
				}
			}
		}()
	}

	// Block forever
	select {}
}

// SaveClip triggers a clip save on the global recorder.
func SaveClip() (string, error) {
	if globalRecorder == nil {
		return "", fmt.Errorf("recorder not initialized")
	}
	return globalRecorder.SaveClip()
}

func KillRecorder() {
	if globalRecorder != nil {
		_ = globalRecorder.Stop()
	}
}

// detectLoopbackDevice and detectMicDevice are copied from setup_linux.go
// They return a PulseAudio device name.
func detectLoopbackDevice(ffmpegBin string, log *zap.Logger) string {
	// Simplified: try pactl info to get default sink monitor
	out, err := exec.Command("pactl", "info").Output()
	if err != nil {
		log.Warn("pactl not available, using default.monitor", zap.Error(err))
		return "default.monitor"
	}
	lines := strings.Split(string(out), "\n")
	for _, line := range lines {
		if strings.HasPrefix(line, "Default Sink:") {
			sink := strings.TrimSpace(strings.TrimPrefix(line, "Default Sink:"))
			if sink != "" {
				return sink + ".monitor"
			}
		}
	}
	return "default.monitor"
}

func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	out, err := exec.Command("pactl", "list", "sources", "short").Output()
	if err != nil {
		log.Warn("pactl not available, mic disabled", zap.Error(err))
		return ""
	}
	lines := strings.Split(string(out), "\n")
	for _, line := range lines {
		fields := strings.Fields(line)
		if len(fields) >= 2 {
			name := fields[1]
			if !strings.Contains(name, ".monitor") {
				return name
			}
		}
	}
	return ""
}

func loopbackInputArgs(device string) []string {
	return []string{}
}

func micInputArgs(device string) []string {
	return []string{}
}
