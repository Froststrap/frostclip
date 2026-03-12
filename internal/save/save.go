package save

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/go-toast/toast"
	"go.uber.org/zap"
	"frostclip/internal/buffer"
	"frostclip/internal/hotkey"
	"frostclip/internal/process"
)

type Config struct {
	FFmpegBin string
	OutputDir string
}

func Handler(buf *buffer.CircularBuffer, saveChan <-chan hotkey.SaveRequest, cfg Config, log *zap.Logger) {
	log = log.Named("save")
	log.Info("save handler ready")
	for req := range saveChan {
		req := req
		go func() {
			log.Info("saving clip", zap.Int("seconds", req.Seconds))
			if err := saveClip(buf, req.Seconds, cfg, log); err != nil {
				log.Error("save failed", zap.Error(err))
			}
		}()
	}
}

func saveClip(buf *buffer.CircularBuffer, seconds int, cfg Config, log *zap.Logger) error {
	segments := buf.GetRecent(seconds)
	if len(segments) == 0 {
		log.Warn("no segments yet — wait a few seconds after launch")
		return nil
	}

	if err := os.MkdirAll(cfg.OutputDir, 0755); err != nil {
		return fmt.Errorf("failed to create output dir: %w", err)
	}

	copyDir, err := os.MkdirTemp("", "frostclip-save-*")
	if err != nil {
		return fmt.Errorf("failed to create save temp dir: %w", err)
	}
	defer os.RemoveAll(copyDir)

	type copyErr struct {
		idx int
		err error
	}
	errs := make(chan copyErr, len(segments))
	dsts := make([]string, len(segments))

	var wg sync.WaitGroup
	for i, src := range segments {
		wg.Add(1)
		i, src := i, src
		dst := filepath.Join(copyDir, fmt.Sprintf("seg_%06d.ts", i))
		dsts[i] = dst
		go func() {
			defer wg.Done()
			if err := copyFile(src, dst); err != nil {
				errs <- copyErr{idx: i, err: err}
			}
		}()
	}
	wg.Wait()
	close(errs)

	failed := make(map[int]bool)
	for ce := range errs {
		log.Warn("could not copy segment",
			zap.String("segment", filepath.Base(segments[ce.idx])),
			zap.Error(ce.err),
		)
		failed[ce.idx] = true
	}

	copied := make([]string, 0, len(segments))
	for i, dst := range dsts {
		if !failed[i] {
			copied = append(copied, dst)
		}
	}

	if len(copied) == 0 {
		return fmt.Errorf("no segments could be read — try again in a moment")
	}

	concatFile, err := os.CreateTemp("", "frostclip-concat-*.txt")
	if err != nil {
		return fmt.Errorf("failed to create concat file: %w", err)
	}
	concatPath := concatFile.Name()
	defer os.Remove(concatPath)

	var sb strings.Builder
	for _, seg := range copied {
		clean := filepath.Clean(seg)
		escaped := strings.ReplaceAll(clean, "'", "'\\''")
		fmt.Fprintf(&sb, "file '%s'\n", escaped)
	}
	if _, err := concatFile.WriteString(sb.String()); err != nil {
		concatFile.Close()
		return fmt.Errorf("failed to write concat file: %w", err)
	}
	concatFile.Close()

	timestamp := time.Now().Format("2006-01-02_15-04-05")
	outputPath := filepath.Join(cfg.OutputDir, fmt.Sprintf("clip_%ds_%s.mp4", seconds, timestamp))

	log.Info("joining segments", zap.Int("count", len(copied)))

	cmd := process.Command(cfg.FFmpegBin,
		"-y",
		"-f", "concat",
		"-safe", "0",
		"-i", concatPath,
		"-c", "copy",
		outputPath,
	)
	output, err := cmd.CombinedOutput()
	if err != nil {
		log.Error("ffmpeg concat failed", zap.String("stderr", string(output)), zap.Error(err))
		return fmt.Errorf("ffmpeg concat failed: %w", err)
	}

	info, _ := os.Stat(outputPath)
	sizeMB := float64(0)
	if info != nil {
		sizeMB = float64(info.Size()) / 1024 / 1024
	}

	log.Info("clip saved",
		zap.Int("seconds", seconds),
		zap.Float64("mb", sizeMB),
	)
	sendNotification(outputPath, seconds, sizeMB)
	return nil
}

func sendNotification(outputPath string, durationSecs int, sizeMB float64) {
	n := toast.Notification{
		AppID:   "FrostClip",
		Title:   "Clip Saved!",
		Message: fmt.Sprintf("%ds clip saved (%.1f MB)\n%s", durationSecs, sizeMB, outputPath),
		Actions: []toast.Action{
			{Type: "protocol", Label: "Open Folder", Arguments: "file:///" + filepath.ToSlash(filepath.Dir(outputPath))},
		},
	}
	n.Push()
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()

	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	return out.Sync()
}
