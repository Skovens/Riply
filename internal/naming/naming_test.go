package naming

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

// fakeProvider records the (query, mediaType) queries it receives and serves
// canned results.
type fakeProvider struct {
	name    string
	enabled bool
	calls   []string
	byType  map[MediaType]func() ([]Result, error)
}

func (f *fakeProvider) Name() string { return f.name }
func (f *fakeProvider) Enabled() bool {
	return f.enabled
}
func (f *fakeProvider) record(q string, mt MediaType) { f.calls = append(f.calls, fmt.Sprintf("%s:%s", mt, q)) }
func (f *fakeProvider) Search(ctx context.Context, query string, mediaType MediaType) ([]Result, error) {
	f.record(query, mediaType)
	if f.byType == nil {
		return nil, nil
	}
	if fn, ok := f.byType[mediaType]; ok {
		return fn()
	}
	return nil, nil
}

func TestParseTitle(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"MASK", "MASK"},
		{"The Mask - S01E01 - Pilot", "The Mask"},
		{"Breaking Bad - S01E01 - Pilot", "Breaking Bad"},
		{"Avatar: The Way of Water", "Avatar: The Way of Water"},
		{"KLOVN - Ep 1", "KLOVN"},
		{"", ""},
		{"Unknown", "Unknown"},
	}
	for _, tt := range tests {
		got := ParseTitle(tt.input)
		if got != tt.want {
			t.Errorf("ParseTitle(%q) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

type failingTransport struct{ err error }

func (f failingTransport) RoundTrip(*http.Request) (*http.Response, error) { return nil, f.err }

// H1: the TMDb endpoint embeds api_key in the query; a transport failure's
// *url.Error must not carry it into logs or API responses, and must still
// match the transport cause via errors.Is.
func TestSearchTransportErrorRedactsAPIKey(t *testing.T) {
	sentinel := errors.New("dial tcp: connect refused")
	p := newTMDBProvider("sup3rsecretkey")
	p.rl.cooldown = 0
	p.client.Transport = failingTransport{err: sentinel}

	_, err := p.Search(context.Background(), "Inception", Movie)
	if err == nil {
		t.Fatal("expected error from failing transport")
	}
	if strings.Contains(err.Error(), "api_key") || strings.Contains(err.Error(), "sup3rsecretkey") {
		t.Errorf("error leaks API key: %v", err)
	}
	if !errors.Is(err, sentinel) {
		t.Errorf("redacted error no longer matches transport cause: %v (%T)", err, err)
	}
	var ue *url.Error
	if !errors.As(err, &ue) {
		t.Fatalf("expected *url.Error, got %T: %v", err, err)
	}
	if strings.Contains(ue.URL, "?") {
		t.Errorf("url.Error URL retains query: %s", ue.URL)
	}
}

// Non-transport errors pass through redactURLError unchanged.
func TestRedactURLErrorPassesThroughOthers(t *testing.T) {
	other := errors.New("plain")
	if got := redactURLError(other); got != other {
		t.Errorf("redactURLError(non-url.Error) = %v, want same error", got)
	}
}

// M1: a slow provider response must be cancellable through ctx — shutdown or
// a gone client cancels the in-flight search instead of blocking for the
// full client timeout.
func TestSearchHonoursContextCancel(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-time.After(5 * time.Second):
		}
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(100 * time.Millisecond)
		cancel()
	}()

	start := time.Now()
	_, err := p.Search(ctx, "Inception", Movie)
	if !errors.Is(err, context.Canceled) {
		t.Errorf("Search with cancelled ctx = %v, want context.Canceled", err)
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Errorf("Search blocked %v after cancel; want prompt cancellation", elapsed)
	}
}

// M1: an already-cancelled context short-circuits before any HTTP.
func TestSearchContextPreCancelled(t *testing.T) {
	calls := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := p.Search(ctx, "Inception", Movie); !errors.Is(err, context.Canceled) {
		t.Errorf("pre-cancelled Search = %v, want context.Canceled", err)
	}
	if calls != 0 {
		t.Errorf("pre-cancelled Search made %d HTTP calls, want 0", calls)
	}
}

// M1: the rate-limit wait must also honour ctx — a cancelled caller is
// released early, it does not sleep the full cooldown.
func TestRateLimitCancelDuringWait(t *testing.T) {
	p := newTMDBProvider("k")
	p.rl.cooldown = 10 * time.Second
	p.rl.wait(context.Background()) // occupy the slot, next call must wait ~10s

	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	start := time.Now()
	err := p.rl.wait(ctx)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Errorf("rateLimit with expiring ctx = %v, want DeadlineExceeded", err)
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Errorf("rateLimit waited %v; want prompt release on ctx done", elapsed)
	}
}

// Malformed TMDb JSON must surface as an error (or empty result), never a
// panic and never a bogus result.
func TestSearchMalformedJSON(t *testing.T) {
	cases := []struct {
		name    string
		body    string
		status  int
		wantErr bool
	}{
		{"truncated json", `{"results":[{"id":27205,"title":"Inc`, 200, true},
		{"html error page", "<html>502 Bad Gateway</html>", 200, true},
		{"empty body", "", 200, true},
		{"wrong types", `{"results": "not-an-array"}`, 200, true},
		{"valid but null results", `{"results": null}`, 200, false},
	}
	for _, tc := range cases {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(tc.status)
			_, _ = w.Write([]byte(tc.body))
		}))
		p := newTMDBProvider("k")
		p.rl.cooldown = 0
		p.baseURL = srv.URL

		results, err := p.Search(context.Background(), "Inception", Movie)
		if tc.wantErr && err == nil {
			t.Errorf("%s: Search(%q) = %v, want error", tc.name, tc.body, results)
		}
		if !tc.wantErr && err != nil {
			t.Errorf("%s: Search error = %v, want nil (null results are valid)", tc.name, err)
		}
		srv.Close()
	}
}

// TMDb queries are media-type-filtered: movie → /search/movie, tv →
// /search/tv, with the type-specific field names mapped.
func TestTMDBTypedEndpoints(t *testing.T) {
	var gotPath, gotType string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		w.Header().Set("Content-Type", "application/json")
		if strings.HasSuffix(gotPath, "/search/tv") {
			_, _ = w.Write([]byte(`{"results":[{"id":7,"name":"Friends","first_air_date":"1994-09-22","poster_path":"/f.jpg"}]}`))
		} else {
			_, _ = w.Write([]byte(`{"results":[{"id":1,"title":"Inception","release_date":"2010-07-16"}]}`))
		}
	}))
	defer srv.Close()

	p := newTMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	movies, err := p.Search(context.Background(), "Inception", Movie)
	if err != nil {
		t.Fatalf("movie search: %v", err)
	}
	if gotPath != "/search/movie" || gotType != "" {
		t.Errorf("movie path = %q", gotPath)
	}
	if len(movies) != 1 || movies[0].Title != "Inception" || movies[0].Year != 2010 || movies[0].IsTV {
		t.Errorf("movies = %+v", movies)
	}
	if movies[0].Provider != "tmdb" || movies[0].ExtID != "1" {
		t.Errorf("movie match ids = %q/%q", movies[0].Provider, movies[0].ExtID)
	}

	gotPath = ""
	tvs, err := p.Search(context.Background(), "Friends", TV)
	if err != nil {
		t.Fatalf("tv search: %v", err)
	}
	if gotPath != "/search/tv" {
		t.Errorf("tv path = %q, want /search/tv", gotPath)
	}
	if len(tvs) != 1 || tvs[0].Title != "Friends" || tvs[0].Year != 1994 || !tvs[0].IsTV {
		t.Errorf("tvs = %+v", tvs)
	}
}

// yearPrefix: numeric prefix parsing for plain years, dates, and TV ranges.
func TestYearPrefix(t *testing.T) {
	cases := []struct {
		in   string
		want int
	}{
		{"2010", 2010},
		{"2010-07-16", 2010},
		{"2010–2013", 2010}, // en-dash
		{"2010-2013", 2010}, // hyphen
		{"", 0},
		{"garbage", 0},
	}
	for _, c := range cases {
		if got := yearPrefix(c.in); got != c.want {
			t.Errorf("yearPrefix(%q) = %d, want %d", c.in, got, c.want)
		}
	}
}

// OMDb: typed searches (?type=movie|series), imdbID → ExtID, year ranges,
// and Response:"False" as an empty (non-error) result set.
func TestOMDBSearch(t *testing.T) {
	var gotQuery url.Values
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotQuery = r.URL.Query()
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"Search":[{"Title":"Friends","Year":"1994–2004","imdbID":"tt0108778","Poster":"https://m.media-amazon.com/p.jpg"}],"Response":"True"}`))
	}))
	defer srv.Close()

	p := newOMDBProvider("omdbkey")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	results, err := p.Search(context.Background(), "Friends", TV)
	if err != nil {
		t.Fatalf("omdb tv search: %v", err)
	}
	if gotQuery.Get("type") != "series" || gotQuery.Get("s") != "Friends" || gotQuery.Get("apikey") != "omdbkey" {
		t.Errorf("omdb query = %v", gotQuery)
	}
	if len(results) != 1 {
		t.Fatalf("results = %+v", results)
	}
	r := results[0]
	if r.Provider != "omdb" || r.ExtID != "tt0108778" || r.Title != "Friends" || r.Year != 1994 || !r.IsTV || r.Poster == "" {
		t.Errorf("omdb result = %+v", r)
	}
}

// OMDb "no match" is a 200 with Response:"False" — empty results, not an
// error, so the chain can fall through to the next provider.
func TestOMDBNoMatchIsEmpty(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"Response":"False","Error":"Movie not found!"}`))
	}))
	defer srv.Close()

	p := newOMDBProvider("k")
	p.rl.cooldown = 0
	p.baseURL = srv.URL

	results, err := p.Search(context.Background(), "Nothing", Movie)
	if err != nil {
		t.Fatalf("omdb no-match error = %v, want nil", err)
	}
	if len(results) != 0 {
		t.Errorf("results = %+v, want empty", results)
	}
}

// OMDb errors must never leak the apikey query parameter.
func TestOMDBTransportErrorRedactsAPIKey(t *testing.T) {
	sentinel := errors.New("dial tcp: connect refused")
	p := newOMDBProvider("s3cretomdbkey")
	p.rl.cooldown = 0
	p.client.Transport = failingTransport{err: sentinel}

	_, err := p.Search(context.Background(), "Friends", TV)
	if err == nil {
		t.Fatal("expected error")
	}
	if strings.Contains(err.Error(), "apikey") || strings.Contains(err.Error(), "s3cretomdbkey") {
		t.Errorf("error leaks API key: %v", err)
	}
}

// --- Chain behaviour ---

func newTestChain(movieOrder, tvOrder []string, ps ...*fakeProvider) *Chain {
	cfgs := make([]ConfiguredProvider, 0, len(ps))
	byName := make(map[string]*fakeProvider, len(ps))
	for _, p := range ps {
		cfgs = append(cfgs, ConfiguredProvider{Name: p.name})
		byName[p.name] = p
	}
	c := New(cfgs, movieOrder, tvOrder)
	// Swap in the fakes so canned behaviours apply (New builds real providers).
	c.providers = make([]Provider, 0, len(ps))
	for _, p := range ps {
		c.providers = append(c.providers, p)
	}
	return c
}

func okResults(title string) func() ([]Result, error) {
	return func() ([]Result, error) {
		return []Result{{Provider: "fake", Title: title}}, nil
	}
}

// Typed Search runs exactly the requested type's order.
func TestChainTypedSearchUsesOrder(t *testing.T) {
	tmdb := &fakeProvider{name: "tmdb", enabled: true}
	omdb := &fakeProvider{name: "omdb", enabled: true, byType: map[MediaType]func() ([]Result, error){Movie: okResults("OMDb Movie")}}
	c := newTestChain([]string{"omdb", "tmdb"}, []string{"tmdb"}, tmdb, omdb)

	results, err := c.Search(context.Background(), "Mask", Movie)
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(results) != 1 || results[0].Title != "OMDb Movie" {
		t.Errorf("results = %+v", results)
	}
	if got := strings.Join(omdb.calls, ","); got != "movie:Mask" {
		t.Errorf("omdb calls = %q", got)
	}
	if len(tmdb.calls) != 0 {
		t.Errorf("tmdb called despite omdb answering first: %v", tmdb.calls)
	}
}

// On error the next provider in the order is tried; on empty results too.
func TestChainFallbackOnErrorAndEmpty(t *testing.T) {
	erring := &fakeProvider{name: "a", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: func() ([]Result, error) { return nil, errors.New("boom") },
	}}
	empty := &fakeProvider{name: "b", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: func() ([]Result, error) { return nil, nil },
	}}
	good := &fakeProvider{name: "c", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: okResults("Good"),
	}}
	c := newTestChain([]string{"a", "b", "c"}, nil, erring, empty, good)

	results, err := c.Search(context.Background(), "q", Movie)
	if err != nil || len(results) != 1 || results[0].Title != "Good" {
		t.Fatalf("Search = (%+v, %v), want Good", results, err)
	}

	// All fail → last error.
	c2 := newTestChain([]string{"a", "b"}, nil, erring, empty)
	if _, err := c2.Search(context.Background(), "q", Movie); err == nil || !strings.Contains(err.Error(), "boom") {
		t.Errorf("all-fail error = %v, want last provider error", err)
	}
}

// Disabled providers (no key) are skipped, not fatal.
func TestChainSkipsDisabled(t *testing.T) {
	disabled := &fakeProvider{name: "a", enabled: false, byType: map[MediaType]func() ([]Result, error){
		Movie: func() ([]Result, error) { return nil, errors.New("must not be called") },
	}}
	good := &fakeProvider{name: "b", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: okResults("Good"),
	}}
	c := newTestChain([]string{"a", "b"}, nil, disabled, good)
	results, err := c.Search(context.Background(), "q", Movie)
	if err != nil || len(results) != 1 {
		t.Fatalf("Search = (%+v, %v)", results, err)
	}
	if len(disabled.calls) != 0 {
		t.Errorf("disabled provider was searched: %v", disabled.calls)
	}
}

// SearchAuto: movie pass first; a short movie pass (errors or empty)
// immediately falls through to the TV pass in the same call.
func TestSearchAutoMovieThenTV(t *testing.T) {
	tmdb := &fakeProvider{name: "tmdb", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: func() ([]Result, error) { return nil, nil }, // "Friends" has no movie
		TV:    okResults("Friends"),
	}}
	c := newTestChain([]string{"tmdb"}, []string{"tmdb"}, tmdb)

	results, err := c.SearchAuto(context.Background(), "Friends")
	if err != nil {
		t.Fatalf("SearchAuto: %v", err)
	}
	if len(results) != 1 || results[0].Title != "Friends" {
		t.Errorf("results = %+v", results)
	}
	if got := strings.Join(tmdb.calls, ","); got != "movie:Friends,tv:Friends" {
		t.Errorf("calls = %q, want movie then tv in one call", got)
	}
}

// SearchAuto never re-runs the same (provider, media-type) query: providers
// unlisted in the movie order still join the movie pass (appended), and the
// TV pass then queries the TV order — a provider may legitimately be queried
// once per type (e.g. TMDb movie-miss then TMDb /search/tv), but not twice
// for the same type.
func TestSearchAutoDistinctQueries(t *testing.T) {
	a := &fakeProvider{name: "a", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: func() ([]Result, error) { return nil, nil },
	}}
	b := &fakeProvider{name: "b", enabled: true, byType: map[MediaType]func() ([]Result, error){
		TV: okResults("FromTV"),
	}}
	c := newTestChain([]string{"a"}, []string{"b"}, a, b)

	results, err := c.SearchAuto(context.Background(), "q")
	if err != nil || len(results) != 1 || results[0].Title != "FromTV" {
		t.Fatalf("SearchAuto = (%+v, %v)", results, err)
	}
	if strings.Join(a.calls, ",") != "movie:q" {
		t.Errorf("a calls = %q, want exactly movie:q (never re-queried)", a.calls)
	}
	if strings.Join(b.calls, ",") != "movie:q,tv:q" {
		t.Errorf("b calls = %q, want movie:q then tv:q (one query per type)", b.calls)
	}
}

// A movie-chain miss must not serve a stale TV-chain result (and vice
// versa): the cache is namespaced by media type.
func TestChainCacheNamespacedByType(t *testing.T) {
	p := &fakeProvider{name: "a", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: okResults("Movie"),
		TV:    okResults("TV"),
	}}
	c := newTestChain([]string{"a"}, []string{"a"}, p)

	if _, err := c.Search(context.Background(), "q", Movie); err != nil {
		t.Fatalf("movie search: %v", err)
	}
	before := len(p.calls)
	tvResults, err := c.Search(context.Background(), "q", TV)
	if err != nil {
		t.Fatalf("tv search: %v", err)
	}
	if tvResults[0].Title != "TV" {
		t.Errorf("tv search returned %q, want fresh TV result (movie cache leaked)", tvResults[0].Title)
	}
	if len(p.calls) != before+1 {
		t.Errorf("tv search made %d extra calls, want 1 (fresh query)", len(p.calls)-before)
	}
	// Repeat searches hit the cache (no new calls).
	if _, err := c.Search(context.Background(), "q", Movie); err != nil {
		t.Fatalf("repeat movie search: %v", err)
	}
	if len(p.calls) != before+1 {
		t.Errorf("repeat movie search made extra calls: %v", p.calls)
	}
}

// SetAPIKey invalidates the cache: results cached under the old key must not
// be served after the configuration changes.
func TestSetAPIKeyClearsCache(t *testing.T) {
	p := newTMDBProvider("k")
	c := New(nil, nil, nil)
	c.providers = []Provider{p}
	c.moviesOrder = []string{"tmdb"}
	c.tvOrder = []string{"tmdb"}
	c.searchCache = make(map[string]searchCacheEntry)
	c.storeSearch(cacheKey("movie", "q"), []Result{{Title: "Stale"}})
	c.SetAPIKey("tmdb", "newkey")
	if _, ok := c.cachedSearch(cacheKey("movie", "q")); ok {
		t.Error("cache survived SetAPIKey")
	}
}

// Registered providers absent from an order list are appended, so a
// configured key is never silently unused.
func TestOrderAppendsUnlistedProviders(t *testing.T) {
	a := &fakeProvider{name: "a", enabled: true}
	b := &fakeProvider{name: "b", enabled: true}
	c := newTestChain([]string{"a"}, []string{"a"}, a, b)
	if got := names(c.orderFor(Movie)); got[0] != "a" || got[1] != "b" {
		t.Errorf("movie order = %v, want a then appended b", got)
	}
}
