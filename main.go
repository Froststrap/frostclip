package main

import (
	"fmt"
	"math"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"
	_ "embed"

	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"

	"frostclip/internal/buffer"
	"frostclip/internal/capture"
	"frostclip/internal/hotkey"
	"frostclip/internal/save"
	"frostclip/internal/setup"
	"frostclip/internal/settings"
	"frostclip/internal/tray"
)

//go:embed froststrap.ico
var iconData []byte

type kvCore struct {
	zapcore.Core
	fields []zapcore.Field
}

func newKVCore(inner zapcore.Core) *kvCore {
	return &kvCore{Core: inner}
}

func (c *kvCore) With(fields []zapcore.Field) zapcore.Core {
	return &kvCore{Core: c.Core, fields: append(c.fields, fields...)}
}

func (c *kvCore) Check(entry zapcore.Entry, ce *zapcore.CheckedEntry) *zapcore.CheckedEntry {
	if c.Core.Enabled(entry.Level) {
		return ce.AddCore(entry, c)
	}
	return ce
}

func (c *kvCore) Write(entry zapcore.Entry, fields []zapcore.Field) error {
	allFields := append(c.fields, fields...)

	var sb strings.Builder
	for i, f := range allFields {
		if i > 0 {
			sb.WriteByte(' ')
		}
		sb.WriteString(f.Key)
		sb.WriteByte('=')
		switch f.Type {
		case zapcore.StringType:
			sb.WriteString(f.String)
		case zapcore.Int64Type, zapcore.Int32Type, zapcore.Int16Type, zapcore.Int8Type:
			fmt.Fprintf(&sb, "%d", f.Integer)
		case zapcore.Uint64Type, zapcore.Uint32Type, zapcore.Uint16Type, zapcore.Uint8Type:
			fmt.Fprintf(&sb, "%d", uint64(f.Integer))
		case zapcore.BoolType:
			if f.Integer == 1 {
				sb.WriteString("true")
			} else {
				sb.WriteString("false")
			}
		case zapcore.ErrorType:
			if f.Interface != nil {
				sb.WriteString(f.Interface.(error).Error())
			}
		case zapcore.Float64Type:
			fmt.Fprintf(&sb, "%.2f", math.Float64frombits(uint64(f.Integer)))
		case zapcore.Float32Type:
			fmt.Fprintf(&sb, "%.2f", math.Float32frombits(uint32(f.Integer)))
		case zapcore.ArrayMarshalerType:
			sb.WriteByte('[')
			if arr, ok := f.Interface.(zapcore.ArrayMarshaler); ok {
				enc := &sliceEncoder{}
				_ = arr.MarshalLogArray(enc)
				sb.WriteString(strings.Join(enc.items, ", "))
			}
			sb.WriteByte(']')
		default:
			fmt.Fprintf(&sb, "%v", f.Interface)
		}
	}

	line := fmt.Sprintf("%s\t%s\t%s",
		entry.Time.Format(time.DateTime),
		entry.Level.CapitalString(),
		entry.Message,
	)
	if sb.Len() > 0 {
		line += "\t" + sb.String()
	}
	line += "\n"

	_, err := fmt.Fprint(os.Stdout, line)
	return err
}

type sliceEncoder struct {
	items []string
}

func (e *sliceEncoder) AppendBool(v bool)             { e.items = append(e.items, fmt.Sprintf("%v", v)) }
func (e *sliceEncoder) AppendByteString(v []byte)     { e.items = append(e.items, string(v)) }
func (e *sliceEncoder) AppendComplex128(v complex128) { e.items = append(e.items, fmt.Sprintf("%v", v)) }
func (e *sliceEncoder) AppendComplex64(v complex64)   { e.items = append(e.items, fmt.Sprintf("%v", v)) }
func (e *sliceEncoder) AppendFloat64(v float64)       { e.items = append(e.items, fmt.Sprintf("%g", v)) }
func (e *sliceEncoder) AppendFloat32(v float32)       { e.items = append(e.items, fmt.Sprintf("%g", v)) }
func (e *sliceEncoder) AppendInt(v int)               { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendInt64(v int64)           { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendInt32(v int32)           { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendInt16(v int16)           { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendInt8(v int8)             { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendString(v string)         { e.items = append(e.items, v) }
func (e *sliceEncoder) AppendUint(v uint)             { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendUint64(v uint64)         { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendUint32(v uint32)         { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendUint16(v uint16)         { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendUint8(v uint8)           { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendUintptr(v uintptr)       { e.items = append(e.items, fmt.Sprintf("%d", v)) }
func (e *sliceEncoder) AppendDuration(v time.Duration) { e.items = append(e.items, v.String()) }
func (e *sliceEncoder) AppendTime(v time.Time)         { e.items = append(e.items, v.Format(time.DateTime)) }
func (e *sliceEncoder) AppendReflected(v any) error {
	e.items = append(e.items, fmt.Sprintf("%v", v))
	return nil
}
func (e *sliceEncoder) AppendObject(v zapcore.ObjectMarshaler) error {
	e.items = append(e.items, fmt.Sprintf("%v", v))
	return nil
}
func (e *sliceEncoder) AppendArray(v zapcore.ArrayMarshaler) error {
	inner := &sliceEncoder{}
	_ = v.MarshalLogArray(inner)
	e.items = append(e.items, "["+strings.Join(inner.items, ", ")+"]")
	return nil
}

func initLogger() (*zap.Logger, error) {
	logsDir := filepath.Join(filepath.Dir(os.Args[0]), "logs")
	if err := os.MkdirAll(logsDir, 0755); err != nil {
		return nil, err
	}

	logPath := filepath.Join(logsDir, "frostclip.log")
	logFile, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return nil, err
	}

	encoderCfg := zapcore.EncoderConfig{
		TimeKey:        "time",
		LevelKey:       "level",
		MessageKey:     "msg",
		EncodeLevel:    zapcore.CapitalLevelEncoder,
		EncodeTime:     zapcore.TimeEncoderOfLayout(time.DateTime),
		EncodeDuration: zapcore.StringDurationEncoder,
	}

	consoleEncoder := zapcore.NewConsoleEncoder(encoderCfg)
	fileEncoder := zapcore.NewConsoleEncoder(encoderCfg)

	fileCore := zapcore.NewCore(fileEncoder, zapcore.AddSync(logFile), zapcore.DebugLevel)
	consoleCore := newKVCore(zapcore.NewCore(consoleEncoder, zapcore.AddSync(os.Stdout), zapcore.DebugLevel))

	return zap.New(zapcore.NewTee(consoleCore, fileCore)), nil
}

func main() {
	log, err := initLogger()
	if err != nil {
		panic("failed to init logger: " + err.Error())
	}
	defer log.Sync()

	log.Info("=== FrostClip - Lightweight Clip Recorder ===")

	cfg, err := settings.Load(log)
	if err != nil {
		log.Fatal("could not load settings.json", zap.Error(err))
	}

	ffmpegBin, err := setup.EnsureFFmpeg(log)
	if err != nil {
		log.Fatal("could not set up FFmpeg", zap.Error(err))
	}

	audioCfg := setup.ResolveAudio(ffmpegBin, cfg.AudioMode, log)

	videosDir := filepath.Join(os.Getenv("USERPROFILE"), "Videos", "FrostClips")
	if err := os.MkdirAll(videosDir, 0755); err != nil {
		log.Fatal("could not create clips directory", zap.Error(err))
	}

	log.Info("FrostClip is running!")
	log.Info("hotkeys: F6=10s  F7=15s  F8=30s  F9=60s  Ctrl+C=quit")

	buf := buffer.New(60)
	saveChan := make(chan hotkey.SaveRequest, 10)

	capCfg := capture.Config{
		FFmpegBin:      ffmpegBin,
		MicDevice:      audioCfg.MicDevice,
		SystemLoopback: audioCfg.SystemLoopback,
		Framerate:      cfg.FPS,
		Resolution:     cfg.Resolution,
		Bitrate:        cfg.Bitrate,
	}
	go capture.Loop(buf, capCfg, log)

	go hotkey.Listen(saveChan, log)

	saveCfg := save.Config{
		FFmpegBin: ffmpegBin,
		OutputDir: videosDir,
	}
	go save.Handler(buf, saveChan, saveCfg, log)

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-quit
		log.Info("signal received — shutting down")
		tray.Quit()
	}()

	tray.Run(saveChan, log, iconData)

	log.Info("shutting down FrostClip...")
	capture.Kill()
	log.Info("FFmpeg process terminated")
	buf.Cleanup()
	log.Info("goodbye")
}
