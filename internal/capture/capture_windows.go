//go:build windows

package capture

import (
	"fmt"
	"os/exec"
	"strings"

	"github.com/yusufpapurcu/wmi"
)

type gpuVendor int

const (
	vendorUnknown gpuVendor = iota
	vendorNVIDIA
	vendorAMD
	vendorIntel
)

type wmiVideoController struct{ Name string }

func detectGPUVendor() gpuVendor {
	var controllers []wmiVideoController
	if err := wmi.Query("SELECT Name FROM Win32_VideoController", &controllers); err != nil {
		return vendorUnknown
	}
	result := vendorUnknown
	for _, c := range controllers {
		name := strings.ToLower(c.Name)
		switch {
		case strings.Contains(name, "nvidia"):
			return vendorNVIDIA
		case strings.Contains(name, "amd"),
			strings.Contains(name, "advanced micro"),
			strings.Contains(name, "radeon"):
			return vendorAMD
		case strings.Contains(name, "intel") && result == vendorUnknown:
			result = vendorIntel
		}
	}
	return result
}

func vendorName(v gpuVendor) string {
	switch v {
	case vendorNVIDIA:
		return "NVIDIA"
	case vendorAMD:
		return "AMD"
	case vendorIntel:
		return "Intel"
	default:
		return "Unknown"
	}
}

func vendorMatchesEncoder(encoder string, vendor gpuVendor) bool {
	switch encoder {
	case "h264_nvenc":
		return vendor == vendorNVIDIA || vendor == vendorUnknown
	case "h264_amf":
		return vendor == vendorAMD || vendor == vendorUnknown
	case "h264_qsv":
		return vendor == vendorIntel || vendor == vendorUnknown
	}
	return true
}

func orderHWByVendor(all []encoderPipeline, vendor gpuVendor) []encoderPipeline {
	preferred := map[gpuVendor]string{
		vendorNVIDIA: "h264_nvenc",
		vendorAMD:    "h264_amf",
		vendorIntel:  "h264_qsv",
	}
	want, ok := preferred[vendor]
	if !ok {
		return all
	}
	var out, rest []encoderPipeline
	for _, p := range all {
		if p.encoder == want {
			out = append(out, p)
		} else {
			rest = append(rest, p)
		}
	}
	return append(out, rest...)
}

func loopbackInputArgs(device string) []string {
	if device == "" {
		device = "default"
	}
	return []string{"-f", "wasapi", "-loopback", "1", "-i", device}
}

func micInputArgs(device string) []string {
	return []string{
		"-f", "dshow",
		"-rtbufsize", "100M",
		"-i", fmt.Sprintf("audio=%s", device),
	}
}

func hwPipelines() []encoderPipeline {
	return []encoderPipeline{
		nvencPipeline(),
		qsvPipeline(),
		amfPipeline(),
	}
}

func scaleFilter(resolution, input, output string) string {
	if resolution == "" {
		return input
	}
	parts := strings.SplitN(resolution, "x", 2)
	if len(parts) != 2 {
		return input
	}
	return fmt.Sprintf("%s,scale=w=%s:h=%s:flags=fast_bilinear%s", input, parts[0], parts[1], output)
}

func nvencPipeline() encoderPipeline {
	return encoderPipeline{
		encoder: "h264_nvenc",
		vendor:  "NVIDIA",
		hw:      true,
		buildArgs: func(cfg Config, segPattern string) []string {
			fps := cfg.Framerate
			if fps == 0 {
				fps = 60
			}
			base := fmt.Sprintf("ddagrab=framerate=%d,hwdownload,format=bgra", fps)
			fc := scaleFilter(cfg.Resolution, base, "") + ",format=yuv420p[vout]"

			args := []string{"-y"}
			inputArgs, audioRefs := audioArgs(cfg)
			args = append(args, inputArgs...)
			br := bitrate(cfg)
			audioFrag, audioLabel := audioMixFragment(audioRefs)
			args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
			if audioLabel != "" {
				args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
			}
			args = append(args,
				"-vcodec", "h264_nvenc",
				"-preset", "p4",
				"-cq", "23",
				"-b:v", "0",
				"-maxrate", br,
				"-bufsize", br,
			)
			return append(args, segmentArgs(segPattern)...)
		},
		probeArgs: func() []string {
			return []string{
				"-filter_complex", "ddagrab=framerate=1,hwdownload,format=bgra,format=yuv420p[vout]",
				"-map", "[vout]",
				"-vcodec", "h264_nvenc", "-preset", "p1", "-cq", "28",
				"-frames:v", "1", "-f", "null", "-",
			}
		},
	}
}

func qsvPipeline() encoderPipeline {
	return encoderPipeline{
		encoder: "h264_qsv",
		vendor:  "Intel",
		hw:      true,
		buildArgs: func(cfg Config, segPattern string) []string {
			fps := cfg.Framerate
			if fps == 0 {
				fps = 60
			}
			fc := fmt.Sprintf("ddagrab=framerate=%d,hwmap=derive_device=qsv:extra_hw_frames=10,format=qsv", fps)
			if cfg.Resolution != "" {
				parts := strings.SplitN(cfg.Resolution, "x", 2)
				if len(parts) == 2 {
					fc += fmt.Sprintf(",scale_qsv=w=%s:h=%s:format=nv12", parts[0], parts[1])
				} else {
					fc += ",scale_qsv=format=nv12"
				}
			} else {
				fc += ",scale_qsv=format=nv12"
			}
			fc += "[vout]"

			args := []string{
				"-y",
				"-init_hw_device", "d3d11va=d3d",
				"-init_hw_device", "qsv=qsv@d3d",
				"-filter_hw_device", "d3d",
			}
			inputArgs, audioRefs := audioArgs(cfg)
			args = append(args, inputArgs...)
			br := bitrate(cfg)
			audioFrag, audioLabel := audioMixFragment(audioRefs)
			args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
			if audioLabel != "" {
				args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
			}
			args = append(args,
				"-vcodec", "h264_qsv",
				"-global_quality", "23",
				"-look_ahead", "0",
				"-maxrate", br,
				"-bufsize", br,
			)
			return append(args, segmentArgs(segPattern)...)
		},
		probeArgs: func() []string {
			return []string{
				"-init_hw_device", "d3d11va=d3d",
				"-init_hw_device", "qsv=qsv@d3d",
				"-filter_hw_device", "d3d",
				"-filter_complex", "ddagrab=framerate=1,hwmap=derive_device=qsv,format=qsv[vout]",
				"-map", "[vout]",
				"-vcodec", "h264_qsv", "-global_quality", "26", "-look_ahead", "0",
				"-frames:v", "1", "-f", "null", "-",
			}
		},
	}
}

func amfPipeline() encoderPipeline {
	return encoderPipeline{
		encoder: "h264_amf",
		vendor:  "AMD",
		hw:      true,
		buildArgs: func(cfg Config, segPattern string) []string {
			fps := cfg.Framerate
			if fps == 0 {
				fps = 60
			}
			base := fmt.Sprintf("ddagrab=framerate=%d,hwdownload,format=bgra", fps)
			fc := scaleFilter(cfg.Resolution, base, "") + ",format=yuv420p[vout]"

			args := []string{"-y"}
			inputArgs, audioRefs := audioArgs(cfg)
			args = append(args, inputArgs...)
			br := bitrate(cfg)
			audioFrag, audioLabel := audioMixFragment(audioRefs)
			args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
			if audioLabel != "" {
				args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
			}
			args = append(args,
				"-vcodec", "h264_amf",
				"-quality", "balanced",
				"-rc", "cbr",
				"-b:v", br,
				"-maxrate", br,
				"-bufsize", br,
			)
			return append(args, segmentArgs(segPattern)...)
		},
		probeArgs: func() []string {
			return []string{
				"-filter_complex", "ddagrab=framerate=1,hwdownload,format=bgra,format=yuv420p[vout]",
				"-map", "[vout]",
				"-vcodec", "h264_amf", "-quality", "speed", "-rc", "cbr", "-b:v", "4000k",
				"-frames:v", "1", "-f", "null", "-",
			}
		},
	}
}

func BuildCaptureCommand(cfg Config, output string) *exec.Cmd {
	vendor := detectGPUVendor()
	pipelines := orderHWByVendor(hwPipelines(), vendor)
	if len(pipelines) == 0 {
		return exec.Command(cfg.FFmpegBin, "-y", "-t", fmt.Sprintf("%d", segmentSeconds), output)
	}
	p := pipelines[0]
	args := p.buildArgs(cfg, output)
	if len(args) > 0 {
		last := args[len(args)-1]
		args = append(args[:len(args)-1], "-t", fmt.Sprintf("%d", segmentSeconds), last)
	}
	return exec.Command(cfg.FFmpegBin, args...)
}

func UseSegmentMuxer() bool { return false }
