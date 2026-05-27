//go:build linux

package capture

import (
	"fmt"
	"os"
	"syscall"

	"golang.org/x/sys/unix"
)

func allocShm(size int) (int, []byte, func(), error) {
	// Preferred: try memfd-backed shm with different flags to maximize compositor compatibility.
	// First try without CLOEXEC and allowing seals, then with CLOEXEC if that fails.
	tryFlags := []int{unix.MFD_ALLOW_SEALING, unix.MFD_CLOEXEC | unix.MFD_ALLOW_SEALING}
	for _, flags := range tryFlags {
		fd, err := unix.MemfdCreate("frostclip-frame", flags)
		if err != nil {
			continue
		}
		// set size
		if err := unix.Ftruncate(fd, int64(size)); err == nil {
			// attempt to add seals (non-fatal)
			seals := unix.F_SEAL_SHRINK | unix.F_SEAL_GROW | unix.F_SEAL_SEAL | unix.F_SEAL_WRITE
			_, _ = unix.FcntlInt(uintptr(fd), unix.F_ADD_SEALS, seals)
			// clear FD_CLOEXEC at fd level to be safest for compositors
			_, _ = unix.FcntlInt(uintptr(fd), unix.F_SETFD, 0)
			data, mmapErr := syscall.Mmap(fd, 0, size, syscall.PROT_READ|syscall.PROT_WRITE, syscall.MAP_SHARED)
			if mmapErr == nil {
				cleanup := func() {
					syscall.Munmap(data)
					unix.Close(fd)
				}
				return fd, data, cleanup, nil
			}
			// mmap failed; close fd and try next option
		}
		unix.Close(fd)
	}

	// Fallback: anonymous tempfile in /dev/shm (unlink immediately)
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
