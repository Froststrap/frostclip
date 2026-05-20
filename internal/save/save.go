package save

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
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
	partPath := outputPath + ".part.mp4"
	defer os.Remove(partPath)

	strategies := saveStrategies()
	var lastErr error
	for _, s := range strategies {
		_ = os.Remove(partPath)
		args := append([]string{
			"-y", "-f", "concat", "-safe", "0", "-i", tmpFile.Name(),
			"-map", "0:v:0", "-map", "0:a?",
		}, s.args...)
		args = append(args, partPath)

		cmd := process.Command(cfg.FFmpegBin, args...)
		if out, err := cmd.CombinedOutput(); err != nil {
			lastErr = err
			log.Debug("save strategy failed",
				zap.String("strategy", s.name),
				zap.Error(err),
				zap.ByteString("out", out),
			)
			continue
		}
		if err := os.Rename(partPath, outputPath); err != nil {
			return err
		}
		log.Info("clip saved!", zap.String("path", outputPath), zap.String("strategy", s.name))
		notify.SendWithAction(
			"Clip Saved",
			fmt.Sprintf("Saved the last %d seconds.", seconds),
			"Folder",
			outputPath,
		)
		return nil
	}
	if lastErr != nil {
		return lastErr
	}
	return fmt.Errorf("no save strategies available")
}

type saveStrategy struct {
	name string
	args []string
}

func saveStrategies() []saveStrategy {
	strategies := []saveStrategy{
		{
			name: "copy-remux",
			args: []string{
				"-c", "copy",
				"-movflags", "+faststart",
			},
		},
	}

	switch runtime.GOOS {
	case "linux":
		strategies = append(strategies, saveStrategy{
			name: "vaapi",
			args: []string{
				"-af", "aresample=async=1:first_pts=0,apad",
				"-init_hw_device", "vaapi=va:/dev/dri/renderD128",
				"-filter_hw_device", "va",
				"-vf", "format=nv12,hwupload",
				"-c:v", "h264_vaapi",
				"-qp", "23",
				"-bf", "0",
				"-c:a", "aac", "-b:a", "128k",
				"-shortest",
				"-movflags", "+faststart",
			},
		})
	case "windows":
		strategies = append(strategies,
			saveStrategy{
				name: "nvenc",
				args: []string{
					"-af", "aresample=async=1:first_pts=0,apad",
					"-c:v", "h264_nvenc", "-preset", "p4", "-cq", "23", "-b:v", "0", "-bf", "0",
					"-pix_fmt", "yuv420p",
					"-c:a", "aac", "-b:a", "128k",
					"-shortest",
					"-movflags", "+faststart",
				},
			},
			saveStrategy{
				name: "qsv",
				args: []string{
					"-af", "aresample=async=1:first_pts=0,apad",
					"-c:v", "h264_qsv", "-global_quality", "23", "-look_ahead", "0", "-bf", "0",
					"-pix_fmt", "yuv420p",
					"-c:a", "aac", "-b:a", "128k",
					"-shortest",
					"-movflags", "+faststart",
				},
			},
			saveStrategy{
				name: "amf",
				args: []string{
					"-af", "aresample=async=1:first_pts=0,apad",
					"-c:v", "h264_amf", "-quality", "speed", "-rc", "cqp", "-qp_i", "23", "-qp_p", "23",
					"-pix_fmt", "yuv420p",
					"-c:a", "aac", "-b:a", "128k",
					"-shortest",
					"-movflags", "+faststart",
				},
			},
		)
	case "darwin":
		strategies = append(strategies, saveStrategy{
			name: "videotoolbox",
			args: []string{
				"-af", "aresample=async=1:first_pts=0,apad",
				"-c:v", "h264_videotoolbox",
				"-pix_fmt", "yuv420p",
				"-c:a", "aac", "-b:a", "128k",
				"-shortest",
				"-movflags", "+faststart",
			},
		})
	}

	strategies = append(strategies, saveStrategy{
		name: "libx264-ultrafast",
		args: []string{
			"-af", "aresample=async=1:first_pts=0,apad",
			"-c:v", "libx264",
			"-preset", "ultrafast",
			"-crf", "23",
			"-pix_fmt", "yuv420p",
			"-bf", "0",
			"-movflags", "+faststart",
			"-c:a", "aac", "-b:a", "128k",
			"-shortest",
		},
	})
	return strategies
}
