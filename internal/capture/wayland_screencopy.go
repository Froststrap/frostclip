//go:build linux

package capture

import (
	"sync"

	"github.com/neurlang/wayland/wl"
)

type WlrScreencopyManagerV1 struct {
	wl.BaseProxy
}

func NewWlrScreencopyManagerV1(ctx *wl.Context) *WlrScreencopyManagerV1 {
	ret := new(WlrScreencopyManagerV1)
	ctx.Register(ret)
	return ret
}

func (p *WlrScreencopyManagerV1) CaptureOutput(output *wl.Output, overlayCursor int32) (*WlrScreencopyFrameV1, error) {
	frame := NewWlrScreencopyFrameV1(p.Context())
	return frame, p.Context().SendRequest(p, 0, frame, overlayCursor, output)
}

func (p *WlrScreencopyManagerV1) Destroy() error {
	return p.Context().SendRequest(p, 2)
}

func (p *WlrScreencopyManagerV1) Dispatch(_ *wl.Event) {}

type WlrScreencopyFrameV1 struct {
	wl.BaseProxy

	mu           sync.Mutex
	BufferFormat uint32
	BufferWidth  uint32
	BufferHeight uint32
	BufferStride uint32
	OnBuffer     func(format, width, height, stride uint32)
	OnBufferDone func()
	OnReady      func()
	OnFailed     func()
	OnFlags      func(flags uint32)
}

func NewWlrScreencopyFrameV1(ctx *wl.Context) *WlrScreencopyFrameV1 {
	ret := new(WlrScreencopyFrameV1)
	ctx.Register(ret)
	return ret
}

func (p *WlrScreencopyFrameV1) Copy(buffer *wl.Buffer) error {
	return p.Context().SendRequest(p, 0, buffer)
}

func (p *WlrScreencopyFrameV1) CopyWithDamage(buffer *wl.Buffer) error {
	return p.Context().SendRequest(p, 2, buffer)
}

func (p *WlrScreencopyFrameV1) Destroy() error {
	return p.Context().SendRequest(p, 1)
}

func (p *WlrScreencopyFrameV1) Dispatch(event *wl.Event) {
	switch event.Opcode {
	case 0: // buffer
		format := event.Uint32()
		width := event.Uint32()
		height := event.Uint32()
		stride := event.Uint32()
		p.mu.Lock()
		p.BufferFormat = format
		p.BufferWidth = width
		p.BufferHeight = height
		p.BufferStride = stride
		cb := p.OnBuffer
		p.mu.Unlock()
		if cb != nil {
			cb(format, width, height, stride)
		}
	case 2: // ready
		_ = event.Uint32() // tv_sec_hi
		_ = event.Uint32() // tv_sec_lo
		_ = event.Uint32() // tv_nsec
		p.mu.Lock()
		cb := p.OnReady
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	case 3: // failed
		p.mu.Lock()
		cb := p.OnFailed
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	case 1: // flags
		flags := event.Uint32()
		p.mu.Lock()
		cb := p.OnFlags
		p.mu.Unlock()
		if cb != nil {
			cb(flags)
		}
	case 6: // buffer_done
		p.mu.Lock()
		cb := p.OnBufferDone
		p.mu.Unlock()
		if cb != nil {
			cb()
		}
	}
}
