package buffer

import (
	"fmt"
	"os"
	"runtime"
	"sync"
	"time"
)

type segment struct {
	path     string
	pushedAt time.Time
}

type CircularBuffer struct {
	mu       sync.Mutex
	segments []segment
	maxSize  int
	head     int
	count    int
	tempDir  string
}

func New(maxSlots int) *CircularBuffer {
	b, err := NewWithBaseDir(maxSlots, "")
	if err != nil {
		panic("failed to create temp dir: " + err.Error())
	}
	return b
}

func NewWithBaseDir(maxSlots int, baseDir string) (*CircularBuffer, error) {
	tmpDir, err := createTempDir(baseDir)
	if err != nil {
		return nil, err
	}
	return &CircularBuffer{
		segments: make([]segment, maxSlots),
		maxSize:  maxSlots,
		tempDir:  tmpDir,
	}, nil
}

func createTempDir(baseDir string) (string, error) {
	if baseDir != "" {
		if err := os.MkdirAll(baseDir, 0o755); err != nil {
			return "", fmt.Errorf("create segment temp dir %q: %w", baseDir, err)
		}
		dir, err := os.MkdirTemp(baseDir, "frostclip-*")
		if err != nil {
			return "", fmt.Errorf("create temp segment dir in %q: %w", baseDir, err)
		}
		return dir, nil
	}

	if runtime.GOOS == "linux" {
		const shmDir = "/dev/shm"
		if info, err := os.Stat(shmDir); err == nil && info.IsDir() {
			if dir, err := os.MkdirTemp(shmDir, "frostclip-*"); err == nil {
				return dir, nil
			}
		}
	}

	dir, err := os.MkdirTemp("", "frostclip-*")
	if err != nil {
		return "", fmt.Errorf("create temp segment dir: %w", err)
	}
	return dir, nil
}

func (b *CircularBuffer) Push(path string) {
	b.mu.Lock()
	defer b.mu.Unlock()

	// One-in, One-out cleanup: Delete the oldest file on disk
	if b.count == b.maxSize && b.segments[b.head].path != "" {
		_ = os.Remove(b.segments[b.head].path)
	}

	b.segments[b.head] = segment{path: path, pushedAt: time.Now()}
	b.head = (b.head + 1) % b.maxSize
	if b.count < b.maxSize {
		b.count++
	}
}

// GetLastNSegments ensures we get the requested duration regardless of time drift
func (b *CircularBuffer) GetLastNSegments(n int) []string {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.count == 0 {
		return nil
	}
	take := n
	if b.count < n {
		take = b.count
	}

	result := make([]string, 0, take)
	start := (b.head - take + b.maxSize) % b.maxSize

	for i := 0; i < take; i++ {
		idx := (start + i) % b.maxSize
		if b.segments[idx].path != "" {
			result = append(result, b.segments[idx].path)
		}
	}
	return result
}

func (b *CircularBuffer) Cleanup() {
	b.mu.Lock()
	defer b.mu.Unlock()
	_ = os.RemoveAll(b.tempDir) // Final cleanup on exit
}

func (b *CircularBuffer) TempDir() string { return b.tempDir }
