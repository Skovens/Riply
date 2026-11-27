package naming

import "context"

// MediaType selects the media-type-filtered variant of a provider search.
// Every provider must support both types.
type MediaType string

const (
	Movie MediaType = "movie"
	TV    MediaType = "tv"
)

// MediaTypeOf maps a classified media type string ("movie", "tv", or
// anything else) to a MediaType. "unknown" has no direct provider query —
// callers route it through Chain.SearchAuto instead.
func MediaTypeOf(s string) (MediaType, bool) {
	switch MediaType(s) {
	case Movie:
		return Movie, true
	case TV:
		return TV, true
	}
	return "", false
}

// Provider is one metadata source. Implementations map their native search
// API onto Result; they never leak their API key into errors (see
// redactURLError).
type Provider interface {
	Name() string
	// Enabled reports whether this provider has a usable API key. Chains
	// skip disabled providers at search time, so a key saved later via
	// Settings takes effect without a restart.
	Enabled() bool
	// Search runs a media-type-filtered query. An empty result set is a
	// normal outcome ("no match"), not an error.
	Search(ctx context.Context, query string, mediaType MediaType) ([]Result, error)
}

// ConfiguredProvider is one provider name plus its configured key, in plain
// parameters so internal/naming never imports internal/config.
type ConfiguredProvider struct {
	Name   string
	APIKey string
}

// keySetter is implemented by providers whose key can change at runtime
// (Settings PUT /api/config).
type keySetter interface {
	SetAPIKey(key string)
}
