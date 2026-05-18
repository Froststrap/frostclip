//go:build linux

// goddamn Wayland

package capture

import (
	"sync"

	"github.com/neurlang/wayland/wl"
)

type ExtImageCaptureSourceV1 struct {
	wl.BaseProxy
}

func NewExtImageCaptureSourceV1(ctx *wl.Context) *ExtImageCaptureSourceV1 {
	ret := new(ExtImageCaptureSourceV1)
	ctx.Register(ret)
	return ret
}

func (p *ExtImageCaptureSourceV1) Destroy() error {
	return p.Context().SendRequest(p, 0)
}

func (p *ExtImageCaptureSourceV1) Dispatch(_ *wl.Event) {}

type ExtOutputImageCaptureSourceManagerV1 struct {
	wl.BaseProxy
}

func NewExtOutputImageCaptureSourceManagerV1(ctx *wl.Context) *ExtOutputImageCaptureSourceManagerV1 {
	ret := new(ExtOutputImageCaptureSourceManagerV1)
	ctx.Register(ret)
	return ret
}

func (p *ExtOutputImageCaptureSourceManagerV1) CreateSource(output *wl.Output) (*ExtImageCaptureSourceV1, error) {
	source := NewExtImageCaptureSourceV1(p.Context())
	return source, p.Context().SendRequest(p, 0, source, output)
}

func (p *ExtOutputImageCaptureSourceManagerV1) Destroy() error {
	return p.Context().SendRequest(p, 1)
}

func (p *ExtOutputImageCaptureSourceManagerV1) Dispatch(_ *wl.Event) {}

type ExtImageCopyCaptureManagerV1 struct {
	wl.BaseProxy
}

func NewExtImageCopyCaptureManagerV1(ctx *wl.Context) *ExtImageCopyCaptureManagerV1 {
	ret := new(ExtImageCopyCaptureManagerV1)
	ctx.Register(ret)
	return ret
}

// CreateSession creates a capture session for a source object.
func (p *ExtImageCopyCaptureManagerV1) CreateSession(source *ExtImageCaptureSourceV1, options uint32) (*ExtImageCopyCaptureSessionV1, error) {
	session := NewExtImageCopyCaptureSessionV1(p.Context())
	return session, p.Context().SendRequest(p, 0, session, source, options)
}

func (p *ExtImageCopyCaptureManagerV1) Destroy() error {
	return p.Context().SendRequest(p, 2)
}

func (p *ExtImageCopyCaptureManagerV1) Dispatch(_ *wl.Event) {}

type ExtImageCopyCaptureSessionV1 struct {
	wl.BaseProxy

	mu           sync.Mutex
	BufferWidth  uint32
	BufferHeight uint32
	ShmFormats   map[uint32]bool
	OnBufferSize func(width, height uint32)
	OnDone       func()
	OnStopped    func()
}

func NewExtImageCopyCaptureSessionV1(ctx *wl.Context) *ExtImageCopyCaptureSessionV1 {
	ret := new(ExtImageCopyCaptureSessionV1)
	ret.ShmFormats = make(map[uint32]bool)
	ctx.Register(ret)
	return ret
}

func (p *ExtImageCopyCaptureSessionV1) CreateFrame() (*ExtImageCopyCaptureFrameV1, error) {
	frame := NewExtImageCopyCaptureFrameV1(p.Context())
	return frame, p.Context().SendRequest(p, 0, frame)
}

func (p *ExtImageCopyCaptureSessionV1) Destroy() error {
	return p.Context().SendRequest(p, 1)
}

func (p *ExtImageCopyCaptureSessionV1) Dispatch(event *wl.Event) {
	switch event.Opcode {
	case 0: // buffer_size
		w := event.Uint32()
		h := event.Uint32()
		p.mu.Lock()
		p.BufferWidth = w
		p.BufferHeight = h
		cb := p.OnBufferSize
		p.mu.Unlock()
		if cb != nil {
			cb(w, h)
		}
	case 1: // shm_format
		format := event.Uint32()
		p.mu.Lock()
		p.ShmFormats[format] = true
		p.mu.Unlock()
	case 4: // done
		p.mu.Lock()
		cb := p.OnDone
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	case 5: // stopped
		p.mu.Lock()
		cb := p.OnStopped
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	}
}

func (p *ExtImageCopyCaptureSessionV1) SupportsShmFormat(format uint32) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.ShmFormats[format]
}

type ExtImageCopyCaptureFrameV1 struct {
	wl.BaseProxy

	mu       sync.Mutex
	OnReady  func()
	OnFailed func(reason uint32)
}

func NewExtImageCopyCaptureFrameV1(ctx *wl.Context) *ExtImageCopyCaptureFrameV1 {
	ret := new(ExtImageCopyCaptureFrameV1)
	ctx.Register(ret)
	return ret
}

func (p *ExtImageCopyCaptureFrameV1) Destroy() error {
	return p.Context().SendRequest(p, 0)
}

// AttachBuffer binds the destination wl_buffer.
func (p *ExtImageCopyCaptureFrameV1) AttachBuffer(buffer *wl.Buffer) error {
	return p.Context().SendRequest(p, 1, buffer)
}

// DamageBuffer marks changed region in buffer coordinates.
func (p *ExtImageCopyCaptureFrameV1) DamageBuffer(x, y, width, height int32) error {
	return p.Context().SendRequest(p, 2, x, y, width, height)
}

// Capture triggers compositor copy into attached buffer.
func (p *ExtImageCopyCaptureFrameV1) Capture() error {
	return p.Context().SendRequest(p, 3)
}

func (p *ExtImageCopyCaptureFrameV1) Dispatch(event *wl.Event) {
	switch event.Opcode {
	case 3: // ready
		p.mu.Lock()
		cb := p.OnReady
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	case 4: // failed
		reason := event.Uint32()
		p.mu.Lock()
		cb := p.OnFailed
		p.mu.Unlock()
		if cb != nil {
			cb(reason)
		}
	}
}
