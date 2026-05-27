package authflow

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"

	"frostclip/internal/userdata"

	"go.uber.org/zap"
)

const backendBase = "https://froststrap-website-backend.onrender.com"

type Profile struct {
	DiscordID       string
	DiscordUsername string
	DiscordEmail    string
}

func AuthURL(returnTo string) string {
	v := url.Values{}
	v.Set("return_to", returnTo)
	return backendBase + "/auth/discord?" + v.Encode()
}

func StartCallbackServer(log *zap.Logger) (string, <-chan string, func(), error) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return "", nil, nil, err
	}

	tokenCh := make(chan string, 1)
	mux := http.NewServeMux()
	mux.HandleFunc("/auth/callback", func(w http.ResponseWriter, r *http.Request) {
		token := strings.TrimSpace(r.URL.Query().Get("token"))
		if token == "" {
			http.Error(w, "missing token", http.StatusBadRequest)
			return
		}
		select {
		case tokenCh <- token:
		default:
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = w.Write([]byte("<!doctype html><html><body><h1>Login complete</h1><p>You can close this tab and return to FrostClip.</p></body></html>"))
	})

	srv := &http.Server{Handler: mux}
	go func() {
		_ = srv.Serve(ln)
	}()

	stop := func() {
		_ = srv.Shutdown(context.Background())
	}

	addr := "http://" + ln.Addr().String() + "/auth/callback"
	log.Info("auth callback server started", zap.String("url", addr))
	return addr, tokenCh, stop, nil
}

func FetchProfile(token string) (*Profile, error) {
	req, err := http.NewRequest(http.MethodGet, backendBase+"/me", nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+token)

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 300 {
		return nil, fmt.Errorf("me request failed: %s", resp.Status)
	}

	var me struct {
		DiscordID   string `json:"discord_id"`
		Username    string `json:"username"`
		Email       string `json:"email"`
		DiscordUser string `json:"discord_username"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&me); err != nil {
		return nil, err
	}

	username := me.Username
	if username == "" {
		username = me.DiscordUser
	}

	return &Profile{
		DiscordID:       me.DiscordID,
		DiscordUsername: username,
		DiscordEmail:    me.Email,
	}, nil
}

func SaveTokenLogin(token string) (*userdata.Data, error) {
	profile, err := FetchProfile(token)
	if err != nil {
		return nil, err
	}
	ud := &userdata.Data{
		DiscordID:       profile.DiscordID,
		DiscordUsername: profile.DiscordUsername,
		DiscordEmail:    profile.DiscordEmail,
		AccessToken:     token,
	}
	if err := userdata.Save(ud); err != nil {
		return nil, err
	}
	return ud, nil
}
