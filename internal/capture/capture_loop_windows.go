//go:build windows

package capture

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"time"

	"frostclip/internal/buffer"

	"go.uber.org/zap"
)

func Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	log.Info("capture: Windows — using ddagrab segment muxer")
	for {
		segPattern := filepath.Join(buf.TempDir(), "seg%06d.ts")
		cmd := BuildCaptureCommand(cfg, segPattern)
		cmd.Stderr = os.Stderr

		if err := cmd.Start(); err != nil {
			log.Warn("capture: start failed", zap.Error(err))
			time.Sleep(time.Second)
			continue
		}
		setActiveCmd(cmd.Process)

		ctx, cancel := context.WithCancel(context.Background())
		go watchSegmentsWindows(ctx, buf, buf.TempDir(), log)

		if err := cmd.Wait(); err != nil {
			log.Warn("capture: ffmpeg exited", zap.Error(err))
		}
		cancel()
		clearActiveCmd()
		time.Sleep(500 * time.Millisecond)
	}
}

func watchSegmentsWindows(ctx context.Context, buf *buffer.CircularBuffer, dir string, log *zap.Logger) {
	seen := map[string]bool{}
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(200 * time.Millisecond):
		}
		entries, _ := os.ReadDir(dir)
		for _, e := range entries {
			name := e.Name()
			if !strings.HasSuffix(name, ".ts") || seen[name] {
				continue
			}
			full := filepath.Join(dir, name)
			if segReady(full) {
				seen[name] = true
				buf.Push(full)
				log.Debug("segment pushed", zap.String("file", name))
			}
		}
	}
}

func segReady(path string) bool {
	s1, err := os.Stat(path)
	if err != nil || s1.Size() == 0 {
		return false
	}
	time.Sleep(120 * time.Millisecond)
	s2, err := os.Stat(path)
	return err == nil && s2.Size() == s1.Size() && s2.Size() > 0
}
