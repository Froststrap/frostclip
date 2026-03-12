package buffer

import (
	"os"
	"sync"
	"time"
)

type segment struct {
	path      string
	pushedAt  time.Time
}

type CircularBuffer struct {
	mu       sync.Mutex
	segments []segment
	maxSize  int
	head     int
	count    int
	tempDir  string
}

func New(maxSeconds int) *CircularBuffer {
	tmpDir, err := os.MkdirTemp("", "frostclip-*")
	if err != nil {
		panic("failed to create temp dir: " + err.Error())
	}
	return &CircularBuffer{
		segments: make([]segment, maxSeconds),
		maxSize:  maxSeconds,
		tempDir:  tmpDir,
	}
}

func (b *CircularBuffer) TempDir() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.tempDir
}

func (b *CircularBuffer) Push(path string) {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.count == b.maxSize && b.segments[b.head].path != "" {
		os.Remove(b.segments[b.head].path)
	}

	b.segments[b.head] = segment{path: path, pushedAt: time.Now()}
	b.head = (b.head + 1) % b.maxSize
	if b.count < b.maxSize {
		b.count++
	}
}

func (b *CircularBuffer) GetRecent(seconds int) []string {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.count == 0 {
		return nil
	}

	cutoff := time.Now().Add(-time.Duration(seconds) * time.Second)

	start := b.head
	if b.count < b.maxSize {
		start = 0
	}

	var result []string
	for i := 0; i < b.count; i++ {
		idx := (start + i) % b.maxSize
		seg := b.segments[idx]
		if seg.path != "" && seg.pushedAt.After(cutoff) {
			result = append(result, seg.path)
		}
	}
	return result
}

func (b *CircularBuffer) GetAll() []string {
	b.mu.Lock()
	defer b.mu.Unlock()

	result := make([]string, 0, b.count)
	if b.count == 0 {
		return result
	}

	start := b.head
	if b.count < b.maxSize {
		start = 0
	}

	for i := 0; i < b.count; i++ {
		idx := (start + i) % b.maxSize
		if b.segments[idx].path != "" {
			result = append(result, b.segments[idx].path)
		}
	}
	return result
}

func (b *CircularBuffer) ResetTempDir() error {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.tempDir != "" {
		os.RemoveAll(b.tempDir)
	}

	tmpDir, err := os.MkdirTemp("", "frostclip-*")
	if err != nil {
		return err
	}

	b.tempDir = tmpDir
	b.segments = make([]segment, b.maxSize)
	b.head = 0
	b.count = 0
	return nil
}

func (b *CircularBuffer) Cleanup() {
	b.mu.Lock()
	defer b.mu.Unlock()
	os.RemoveAll(b.tempDir)
}
