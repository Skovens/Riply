package sse

import (
	"sync"
	"testing"
	"time"
)

func TestPublishReachesSubscribers(t *testing.T) {
	m := New()
	a, b := m.Subscribe(), m.Subscribe()

	want := Event{Type: "rip_complete", Drive: "/dev/sr0", Message: "done"}
	m.Publish(want)

	for i, sub := range []*Subscriber{a, b} {
		select {
		case got := <-sub.Chan():
			if got != want {
				t.Errorf("subscriber %d got %+v, want %+v", i, got, want)
			}
		case <-time.After(time.Second):
			t.Fatalf("subscriber %d did not receive published event", i)
		}
	}
}

// The subscriber channel is bounded; a client that stops reading must NOT
// stall the publisher (the HTTP/SSE pump relies on Publish returning
// promptly). Overflow events are dropped, the buffered head is kept.
func TestPublishDropsForSlowSubscriber(t *testing.T) {
	m := New()
	sub := m.Subscribe() // cap 64, never read

	// 200 publishes over the cap; any blocking would hang the test.
	for i := 0; i < 200; i++ {
		m.Publish(Event{Type: "rip_progress", Message: string(rune('a' + i%26))})
	}

	got := 0
	for {
		select {
		case _, ok := <-sub.Chan():
			if !ok {
				t.Fatalf("channel closed unexpectedly after %d events", got)
			}
			got++
		default:
			if got != 64 {
				t.Errorf("received %d events, want exactly the 64-event buffer (drops must keep head)", got)
			}
			return
		}
	}
}

func TestUnsubscribeClosesAndIsIdempotent(t *testing.T) {
	m := New()
	sub := m.Subscribe()
	m.Unsubscribe(sub)
	if !sub.Closed() {
		t.Error("Closed() false after Unsubscribe")
	}
	if _, ok := <-sub.Chan(); ok {
		t.Error("channel still open after Unsubscribe")
	}
	m.Unsubscribe(sub) // second call must not panic (double close)
}

func TestCloseAllClosesEverySubscriber(t *testing.T) {
	m := New()
	subs := []*Subscriber{m.Subscribe(), m.Subscribe(), m.Subscribe()}
	m.CloseAll()
	for i, sub := range subs {
		if !sub.Closed() {
			t.Errorf("subscriber %d not Closed after CloseAll", i)
		}
		if _, ok := <-sub.Chan(); ok {
			t.Errorf("subscriber %d channel still open after CloseAll", i)
		}
	}
	// Unsubscribe after CloseAll must be safe (CAS fails, no double close).
	m.Unsubscribe(subs[0])
	// Publishing into an empty registry is a no-op.
	m.Publish(Event{Type: "orphan"})
}

// Concurrent subscribe/unsubscribe/publish is the actual production pattern
// (SSE handler goroutines + rip pumps). Run under -race.
func TestConcurrentSubscribePublishUnsubscribe(t *testing.T) {
	m := New()
	var wg sync.WaitGroup
	stop := make(chan struct{})

	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				select {
				case <-stop:
					return
				default:
					sub := m.Subscribe()
					for j := 0; j < 10; j++ {
						select {
						case <-sub.Chan():
						case <-time.After(5 * time.Millisecond):
						}
					}
					m.Unsubscribe(sub)
				}
			}
		}()
	}
	wg.Add(1)
	go func() {
		defer wg.Done()
		for {
			select {
			case <-stop:
				return
			default:
				m.Publish(Event{Type: "drive_ready"})
			}
		}
	}()

	time.Sleep(100 * time.Millisecond)
	close(stop)
	wg.Wait()
	m.mu.RLock()
	n := len(m.subscribers)
	m.mu.RUnlock()
	if n != 0 {
		t.Errorf("%d subscribers leaked after teardown", n)
	}
}
