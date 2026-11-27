package naming

import (
	"context"
	"sync"
	"time"
	"testing"
)

// A Settings save (SetOrders/SetAPIKey) concurrent with a search must not
// race: orderFor reads the order lists on every search. Run with -race.
func TestChainOrdersRaceUnderConcurrency(t *testing.T) {
	p := &fakeProvider{name: "tmdb", enabled: true, byType: map[MediaType]func() ([]Result, error){
		Movie: okResults("M"),
		TV:    okResults("T"),
	}}
	c := newTestChain([]string{"tmdb"}, []string{"tmdb"}, p)

	var wg sync.WaitGroup
	stop := make(chan struct{})
	wg.Add(1)
	go func() {
		defer wg.Done()
		for {
			select {
			case <-stop:
				return
			default:
			}
			c.SetOrders([]string{"tmdb"}, []string{"tmdb"})
		}
	}()
	deadline := time.After(200 * time.Millisecond)
	for {
		select {
		case <-deadline:
			close(stop)
			wg.Wait()
			return
		default:
		}
		if _, err := c.Search(context.Background(), "q", Movie); err != nil {
			t.Fatalf("Search: %v", err)
		}
	}
}
