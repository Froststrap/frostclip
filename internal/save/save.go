package save

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"frostclip/internal/buffer"
	"frostclip/internal/capture"
	"frostclip/internal/hotkey"
	"frostclip/internal/notify"
	"frostclip/internal/process"

	"go.uber.org/zap"
)

type Config struct {
	FFmpegBin string
	OutputDir string
}

func Handler(buf *buffer.CircularBuffer, saveChan <-chan hotkey.SaveRequest, cfg Config, log *zap.Logger) {
	log = log.Named("save")
	for req := range saveChan {
		go func(r hotkey.SaveRequest) {
			if err := saveClip(buf, r.Seconds, cfg, log); err != nil {
				log.Error("save failed", zap.Error(err))
				notify.Send("FrostClip Error", fmt.Sprintf("Failed to save %ds clip", r.Seconds))
			}
		}(req)
	}
}

func saveClip(buf *buffer.CircularBuffer, seconds int, cfg Config, log *zap.Logger) error {
	segDur := capture.SegmentDuration
	if segDur <= 0 {
		segDur = 1
	}
	segCount := (seconds + segDur - 1) / segDur

	segments := buf.GetLastNSegments(segCount)
	if len(segments) == 0 {
		return fmt.Errorf("no segments available")
	}

	tmpFile, err := os.CreateTemp("", "frostclip-concat-*.txt")
	if err != nil {
		return err
	}
	defer os.Remove(tmpFile.Name())

	for _, seg := range segments {
		fmt.Fprintf(tmpFile, "file '%s'\n", strings.ReplaceAll(seg, "'", "'\\''"))
	}
	tmpFile.Close()

	timestamp := time.Now().Format("2006-01-02_15-04-05")
	outputPath := filepath.Join(cfg.OutputDir, fmt.Sprintf("clip_%ds_%s.mp4", seconds, timestamp))

	cmd := process.Command(cfg.FFmpegBin,
		"-y", "-f", "concat", "-safe", "0", "-i", tmpFile.Name(),
		"-map", "0:v:0", "-map", "0:a?",
		"-c:v", "libx264",
		"-preset", "veryfast",
		"-crf", "20",
		"-pix_fmt", "yuv420p",
		"-bf", "0",
		"-movflags", "+faststart",
		"-c:a", "aac", "-b:a", "128k",
		outputPath,
	)

	if out, err := cmd.CombinedOutput(); err != nil {
		log.Error("ffmpeg error", zap.ByteString("out", out))
		return err
	}

	log.Info("clip saved!", zap.String("path", outputPath))

	notify.SendWithAction(
		"Clip Saved",
		fmt.Sprintf("Saved the last %d seconds.", seconds),
		"Folder",
		outputPath,
	)

	return nil
}
