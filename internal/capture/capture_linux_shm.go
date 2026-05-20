//go:build linux

package capture

import (
	"fmt"
	"os"
	"syscall"
)

func allocShm(size int) (int, []byte, func(), error) {
	// Use anonymous file-backed shm for compositor compatibility.
	// Some wlroots compositors are stricter with memfd buffers.
	f, err := os.CreateTemp("/dev/shm", "frostclip-frame-*")
	if err != nil {
		f, err = os.CreateTemp("", "frostclip-frame-*")
		if err != nil {
			return -1, nil, nil, fmt.Errorf("create shm tmpfile: %w", err)
		}
	}
	os.Remove(f.Name()) // unlink; fd keeps it alive

	if err := f.Truncate(int64(size)); err != nil {
		f.Close()
		return -1, nil, nil, fmt.Errorf("truncate shm: %w", err)
	}

	data, err := syscall.Mmap(
		int(f.Fd()),
		0,
		size,
		syscall.PROT_READ|syscall.PROT_WRITE,
		syscall.MAP_SHARED,
	)
	if err != nil {
		f.Close()
		return -1, nil, nil, fmt.Errorf("mmap shm: %w", err)
	}

	cleanup := func() {
		syscall.Munmap(data)
		f.Close()
	}

	return int(f.Fd()), data, cleanup, nil
}
