package naming

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"time"

	"riply/internal/logger"
)

// omdbProvider is the OMDb (omdbapi.com) adapter. The free tier allows 1,000
// requests/day; the limiter spaces requests conservatively (1s) — a constant
// here, never a user-facing config field.
type omdbProvider struct {
	httpProvider
}

func newOMDBProvider(apiKey string) *omdbProvider {
	return &omdbProvider{httpProvider{
		apiKey:  apiKey,
		baseURL: "https://www.omdbapi.com",
		client:  &http.Client{Timeout: 10 * time.Second, Transport: userAgentRoundTripper{next: http.DefaultTransport}},
		rl:      rateLimiter{cooldown: 1 * time.Second},
	}}
}

func (p *omdbProvider) Name() string    { return "omdb" }
func (p *omdbProvider) Enabled() bool   { return p.hasKey() }
func (p *omdbProvider) SetAPIKey(key string) { p.setAPIKey(key) }

// omdbNA maps OMDb's literal "N/A" placeholder values (missing poster etc.)
// to empty strings so the frontend never renders a broken image.
func omdbNA(s string) string {
	if s == "N/A" {
		return ""
	}
	return s
}

// Search runs a media-type-filtered OMDb query:
// ?apikey=<key>&s=<query>&type=movie|series. Search responses carry no plot,
// so Overview stays empty; a per-result detail call is a possible follow-up.
func (p *omdbProvider) Search(ctx context.Context, query string, mediaType MediaType) ([]Result, error) {
	if !p.Enabled() {
		return nil, fmt.Errorf("OMDb API key not set")
	}

	omdbType := "movie"
	if mediaType == TV {
		omdbType = "series"
	}

	params := url.Values{}
	params.Set("apikey", p.key())
	params.Set("s", query)
	params.Set("type", omdbType)

	endpoint := fmt.Sprintf("%s/?%s", p.baseURL, params.Encode())
	resp, err := p.get(ctx, p.Name(), endpoint)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, p.errorForBody(p.Name(), resp)
	}

	var data struct {
		Search []struct {
			Title  string `json:"Title"`
			Year   string `json:"Year"` // "2010" or "2010–2013"
			ImdbID string `json:"imdbID"`
			Poster string `json:"Poster"`
		} `json:"Search"`
		Response string `json:"Response"` // "True"/"False"
		Error    string `json:"Error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	// OMDb reports "no match" with HTTP 200 + Response:"False"; that is an
	// empty result set, not an error.
	if data.Response == "False" {
		return nil, nil
	}

	results := make([]Result, 0, len(data.Search))
	for _, r := range data.Search {
		results = append(results, Result{
			Provider: "omdb",
			ExtID:    omdbNA(r.ImdbID),
			Title:    omdbNA(r.Title),
			Year:     yearPrefix(r.Year),
			Poster:   omdbNA(r.Poster),
			IsTV:     mediaType == TV,
		})
	}

	logger.Info("OMDb search results", "type", string(mediaType), "count", len(results), "query", query)
	return results, nil
}
