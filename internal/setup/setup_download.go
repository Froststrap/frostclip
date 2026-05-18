package setup

import (
	"archive/zip"
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

func appDir() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	return filepath.Dir(exe), nil
}

func bundledFFmpegPath() (string, bool) {
	dir, err := appDir()
	if err != nil {
		return "", false
	}
	name := "ffmpeg"
	if runtime.GOOS == "windows" {
		name = "ffmpeg.exe"
	}
	candidates := []string{
		filepath.Join(dir, "ffmpeg", name),
		filepath.Join(dir, "ffmpeg", "bin", name),
	}
	for _, p := range candidates {
		if st, err := os.Stat(p); err == nil && !st.IsDir() {
			return p, true
		}
	}
	return "", false
}

func downloadBytes(url string) ([]byte, error) {
	client := &http.Client{Timeout: 60 * time.Second}
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "FrostClip/1.0")
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("download failed (%s): %s", url, resp.Status)
	}
	return io.ReadAll(resp.Body)
}

func ensureDir(path string) error {
	return os.MkdirAll(path, 0o755)
}

func writeExecutable(path string, data []byte) error {
	if err := ensureDir(filepath.Dir(path)); err != nil {
		return err
	}
	if err := os.WriteFile(path, data, 0o755); err != nil {
		return err
	}
	return os.Chmod(path, 0o755)
}

func unzipFind(data []byte, match func(name string) bool) ([]byte, string, error) {
	r, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return nil, "", err
	}
	for _, f := range r.File {
		name := strings.ToLower(filepath.ToSlash(f.Name))
		if f.FileInfo().IsDir() || !match(name) {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return nil, "", err
		}
		b, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			return nil, "", err
		}
		return b, f.Name, nil
	}
	return nil, "", fmt.Errorf("matching file not found in zip")
}
