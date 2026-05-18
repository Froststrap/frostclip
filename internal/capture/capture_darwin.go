//go:build darwin

package capture

import (
	"fmt"
	"os/exec"
	"strings"
)

func loopbackInputArgs() []string {
	return []string{
		"-f", "avfoundation",
		"-i", ":0",
	}
}

func micInputArgs(device string) []string {
	return []string{
		"-f", "avfoundation",
		"-i", fmt.Sprintf(":%s", device),
	}
}

func BuildCaptureCommand(cfg Config, output string) *exec.Cmd {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}

	args := []string{
		"-y",
		"-f", "avfoundation",
		"-framerate", fmt.Sprintf("%d", fps),
		"-capture_cursor", "0",
		"-i", "1:",
	}

	inputArgs, audioRefs := audioArgs(cfg)
	args = append(args, inputArgs...)

	scale := ""
	if cfg.Resolution != "" {
		parts := strings.SplitN(cfg.Resolution, "x", 2)
		if len(parts) == 2 {
			scale = fmt.Sprintf(",scale=w=%s:h=%s:flags=fast_bilinear", parts[0], parts[1])
		}
	}
	fc := fmt.Sprintf("0:v%s,format=yuv420p[vout]", scale)

	audioFrag, audioLabel := audioMixFragment(audioRefs)
	args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
	if audioLabel != "" {
		args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
	}

	br := bitrate(cfg)
	args = append(args,
		"-vcodec", "h264_videotoolbox",
		"-q:v", "60",
		"-maxrate", br,
		"-bufsize", br,
		"-t", fmt.Sprintf("%d", SegmentDuration),
		output,
	)

	return exec.Command(cfg.FFmpegBin, args...)
}

func buildSegmentCommand(cfg Config, segPattern string) *exec.Cmd {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}

	args := []string{
		"-y",
		"-f", "avfoundation",
		"-framerate", fmt.Sprintf("%d", fps),
		"-capture_cursor", "0",
		"-i", "1:",
	}

	inputArgs, audioRefs := audioArgs(cfg)
	args = append(args, inputArgs...)

	scale := ""
	if cfg.Resolution != "" {
		parts := strings.SplitN(cfg.Resolution, "x", 2)
		if len(parts) == 2 {
			scale = fmt.Sprintf(",scale=w=%s:h=%s:flags=fast_bilinear", parts[0], parts[1])
		}
	}
	fc := fmt.Sprintf("0:v%s,format=yuv420p[vout]", scale)

	audioFrag, audioLabel := audioMixFragment(audioRefs)
	args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
	if audioLabel != "" {
		args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
	}

	br := bitrate(cfg)
	args = append(args,
		"-vcodec", "h264_videotoolbox",
		"-q:v", "60",
		"-maxrate", br,
		"-bufsize", br,
		"-f", "segment",
		"-segment_time", fmt.Sprintf("%d", segmentSeconds),
		"-segment_format", "mpegts",
		"-reset_timestamps", "1",
		segPattern,
	)

	return exec.Command(cfg.FFmpegBin, args...)
}
