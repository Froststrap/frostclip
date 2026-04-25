//go:build linux

package capture

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

type gpuVendor int

const (
	vendorUnknown gpuVendor = iota
	vendorNVIDIA
	vendorAMD
	vendorIntel
)

func detectGPUVendor() gpuVendor {
	entries, err := os.ReadDir("/sys/class/drm")
	if err != nil {
		return vendorUnknown
	}
	for _, e := range entries {
		data, err := os.ReadFile(fmt.Sprintf("/sys/class/drm/%s/device/vendor", e.Name()))
		if err != nil {
			continue
		}
		vendorID := strings.TrimSpace(string(data))
		switch vendorID {
		case "0x10de":
			return vendorNVIDIA
		case "0x1002":
			return vendorAMD
		case "0x8086":
			return vendorIntel
		}
	}
	return vendorUnknown
}

func detectRenderNode() string {
	nodes, err := filepath.Glob("/dev/dri/renderD*")
	if err == nil && len(nodes) > 0 {
		return nodes[0]
	}
	return "/dev/dri/renderD128"
}

func BuildCaptureCommand(cfg Config, output string) *exec.Cmd {
	vendor := detectGPUVendor()

	if os.Getenv("WAYLAND_DISPLAY") != "" {
		if wfPath, err := exec.LookPath("wf-recorder"); err == nil {
			args := []string{"-s", "SIGINT", "1.4s", wfPath}
			args = append(args, wfRecorderArgs(cfg, output, vendor)...)

			cmd := exec.Command("timeout", args...)
			cmd.Stderr = os.Stderr
			return cmd
		}
	}

	cmd := exec.Command(cfg.FFmpegBin, ffmpegX11Args(cfg, output, vendor)...)
	cmd.Stderr = os.Stderr
	return cmd
}

func wfRecorderArgs(cfg Config, output string, v gpuVendor) []string {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}

	res := cfg.Resolution
	if res == "" || res == "1366x768" {
		res = "0,0 1360x768"
	} else if !strings.Contains(res, " ") {
		res = "0,0 " + res
	}

	args := []string{
		"-f", output,
		"-r", fmt.Sprintf("%d", fps),
		"-g", res,
	}

	if cfg.MicDevice != "" {
		args = append(args, "-a", cfg.MicDevice)
	}

	switch v {
	case vendorNVIDIA:
		args = append(args, "-c", "h264_nvenc")
	case vendorAMD, vendorIntel:
		args = append(args, "-c", "h264_vaapi", "-d", detectRenderNode())
	default:
		args = append(args, "-c", "libx264", "-p", "preset=ultrafast")
	}

	return args
}

func ffmpegX11Args(cfg Config, output string, v gpuVendor) []string {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}
	display := os.Getenv("DISPLAY")
	if display == "" {
		display = ":0"
	}

	args := []string{
		"-y",
		"-f", "x11grab",
		"-framerate", fmt.Sprintf("%d", fps),
		"-i", display,
		"-t", "1",
	}

	switch v {
	case vendorNVIDIA:
		args = append(args, "-c:v", "h264_nvenc")
	case vendorAMD, vendorIntel:
		args = append(args, "-vaapi_device", detectRenderNode(), "-vf", "format=nv12,hwupload", "-c:v", "h264_vaapi")
	default:
		args = append(args, "-c:v", "libx264", "-preset", "ultrafast")
	}

	args = append(args, output)
	return args
}
