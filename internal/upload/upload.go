package upload

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"frostclip/internal/clipboard"

	"go.uber.org/zap"
)

const apiURL = "https://froststrap-website-backend.onrender.com/"

type uploadURLResponse struct {
	UploadID  string `json:"upload_id"`
	UploadURL string `json:"upload_url"`
}

type webhookReadyResponse struct {
	ClipURL string `json:"clip_url"`
}

// ClipToAPI uploads a saved clip file to FrostClip.
// Returns the shareable clip URL on success.
func ClipToAPI(clipPath string, accessToken string, log *zap.Logger) (string, error) {
	log = log.Named("upload")
	log.Info("requesting upload URL", zap.String("clip", clipPath))

	uploadInfo, err := requestUploadURL(accessToken)
	if err != nil {
		return "", fmt.Errorf("failed to get upload URL: %w", err)
	}
	log.Info("got upload URL", zap.String("upload_id", uploadInfo.UploadID))

	if err := streamFileTo(clipPath, uploadInfo.UploadURL); err != nil {
		return "", fmt.Errorf("failed to stream clip to Mux: %w", err)
	}
	log.Info("clip streamed to Mux, waiting for processing", zap.String("upload_id", uploadInfo.UploadID))

	// Poll our API for the clip URL (Mux processes asynchronously)
	clipURL, err := pollForClipURL(uploadInfo.UploadID, accessToken, log)
	if err != nil {
		return "", fmt.Errorf("failed to get clip URL: %w", err)
	}

	return clipURL, nil
}

func ClipToAPIAndCopy(clipPath string, accessToken string, log *zap.Logger) (string, bool, error) {
	clipURL, err := ClipToAPI(clipPath, accessToken, log)
	if err != nil {
		return "", false, err
	}

	if err := clipboard.Write(clipURL); err != nil {
		log.Warn("failed to copy clip URL to clipboard", zap.Error(err))
		return clipURL, false, nil
	}

	return clipURL, true, nil
}

func requestUploadURL(accessToken string) (*uploadURLResponse, error) {
	client := &http.Client{Timeout: 15 * time.Second}
	req, err := http.NewRequest(http.MethodPost, apiURL+"/clips/upload", bytes.NewReader([]byte("{}")))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+accessToken)

	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusUnauthorized {
		return nil, fmt.Errorf("not logged in — please log in to FrostClip first")
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("API returned %d: %s", resp.StatusCode, string(body))
	}

	var result uploadURLResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode API response: %w", err)
	}
	if result.UploadURL == "" {
		return nil, fmt.Errorf("API returned empty upload URL")
	}
	return &result, nil
}

func streamFileTo(filePath, uploadURL string) error {
	f, err := os.Open(filePath)
	if err != nil {
		return fmt.Errorf("could not open clip: %w", err)
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return fmt.Errorf("could not stat clip: %w", err)
	}

	client := &http.Client{Timeout: 10 * time.Minute}
	req, err := http.NewRequest(http.MethodPut, uploadURL, f)
	if err != nil {
		return err
	}
	req.ContentLength = info.Size()
	req.Header.Set("Content-Type", "video/mp4")

	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("Mux upload returned %d: %s", resp.StatusCode, string(body))
	}
	return nil
}

// pollForClipURL polls the API until the clip is ready or times out.
func pollForClipURL(uploadID, accessToken string, log *zap.Logger) (string, error) {
	client := &http.Client{Timeout: 10 * time.Second}
	deadline := time.Now().Add(2 * time.Minute)
	interval := 3 * time.Second

	for time.Now().Before(deadline) {
		time.Sleep(interval)

		req, err := http.NewRequest(http.MethodGet,
			fmt.Sprintf("%s/clips/upload/%s/status", apiURL, uploadID), nil)
		if err != nil {
			return "", err
		}
		req.Header.Set("Authorization", "Bearer "+accessToken)

		resp, err := client.Do(req)
		if err != nil {
			log.Warn("poll failed, retrying", zap.Error(err))
			continue
		}

		if resp.StatusCode == http.StatusAccepted {
			// Still processing
			resp.Body.Close()
			log.Debug("clip still processing...")
			continue
		}

		if resp.StatusCode == http.StatusOK {
			var result webhookReadyResponse
			if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
				resp.Body.Close()
				return "", fmt.Errorf("failed to decode clip URL response: %w", err)
			}
			resp.Body.Close()
			return result.ClipURL, nil
		}

		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		return "", fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	return "", fmt.Errorf("timed out waiting for clip to be ready")
}
