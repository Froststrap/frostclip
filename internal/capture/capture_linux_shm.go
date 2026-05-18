//go:build linux

package capture

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

func memfdCreate(name string, flags uint) (uintptr, uintptr, syscall.Errno) {
	nameBytes, err := syscall.BytePtrFromString(name)
	if err != nil {
		return 0, 0, syscall.EINVAL
	}
	r0, r1, errno := syscall.Syscall(
		319,
		uintptr(unsafe.Pointer(nameBytes)),
		uintptr(flags),
		0,
	)
	return r0, r1, errno
}

func allocShm(size int) (int, []byte, func(), error) {
	fd, _, errno := memfdCreate("frostclip-frame", 0)
	var f *os.File
	if errno == 0 {
		f = os.NewFile(fd, "memfd:frostclip-frame")
	} else {
		// Fallback: anonymous file in /dev/shm
		var err error
		f, err = os.CreateTemp("/dev/shm", "frostclip-frame-*")
		if err != nil {
			f, err = os.CreateTemp("", "frostclip-frame-*")
			if err != nil {
				return -1, nil, nil, fmt.Errorf("create shm tmpfile: %w", err)
			}
		}
		os.Remove(f.Name()) // unlink; fd keeps it alive
	}

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
