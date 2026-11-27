package naming

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"riply/internal/logger"
)

// tmdbProvider is the TMDb adapter. Rate limit (1 req/s) and Retry-After
// handling follow TMDb's published limits; they are constants here, not
// configuration.
type tmdbProvider struct {
	httpProvider
}

func newTMDBProvider(apiKey string) *tmdbProvider {
	return &tmdbProvider{httpProvider{
		apiKey:  apiKey,
		baseURL: "https://api.themoviedb.org/3",
		client:  &http.Client{Timeout: 10 * time.Second, Transport: userAgentRoundTripper{next: http.DefaultTransport}},
		rl:      rateLimiter{cooldown: 1 * time.Second},
	}}
}

func (p *tmdbProvider) Name() string    { return "tmdb" }
func (p *tmdbProvider) Enabled() bool   { return p.hasKey() }
func (p *tmdbProvider) SetAPIKey(key string) { p.setAPIKey(key) }

// Search runs a media-type-filtered TMDb query: /search/movie for movies
// (title, release_date) and /search/tv for TV (name, first_air_date) — the
// mixed search/multi endpoint is gone.
func (p *tmdbProvider) Search(ctx context.Context, query string, mediaType MediaType) ([]Result, error) {
	if !p.Enabled() {
		return nil, fmt.Errorf("TMDb API key not set")
	}

	params := url.Values{}
	params.Set("api_key", p.key())
	params.Set("query", query)
	params.Set("include_adult", "false")
	params.Set("page", "1")

	endpoint := fmt.Sprintf("%s/search/%s?%s", p.baseURL, mediaType, params.Encode())
	resp, err := p.get(ctx, p.Name(), endpoint)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, p.errorForBody(p.Name(), resp)
	}

	var data struct {
		Results []struct {
			ID           int    `json:"id"`
			Title        string `json:"title"`  // /search/movie
			Name         string `json:"name"`   // /search/tv
			ReleaseDate  string `json:"release_date"`
			FirstAirDate string `json:"first_air_date"`
			PosterPath   string `json:"poster_path"`
			Overview     string `json:"overview"`
		} `json:"results"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}

	results := make([]Result, 0, len(data.Results))
	for _, r := range data.Results {
		title, date := r.Title, r.ReleaseDate
		if mediaType == TV {
			title, date = r.Name, r.FirstAirDate
		}
		poster := ""
		if r.PosterPath != "" {
			poster = fmt.Sprintf("https://image.tmdb.org/t/p/w200%s", r.PosterPath)
		}
		results = append(results, Result{
			ID:       r.ID,
			ExtID:    strconv.Itoa(r.ID),
			Provider: "tmdb",
			Title:    title,
			Year:     yearPrefix(date),
			Poster:   poster,
			Overview: r.Overview,
			IsTV:     mediaType == TV,
		})
	}

	logger.Info("TMDb search results", "type", string(mediaType), "count", len(results), "query", query)
	if len(results) == 0 {
		logger.Warn("TMDb search: no results", "query", query)
	}
	return results, nil
}

