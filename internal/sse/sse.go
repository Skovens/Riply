package sse

import (
	"sync"
	"sync/atomic"

	"riply/internal/logger"
)

// Event represents an SSE message.
type Event struct {
	Type     string    `json:"type"`
	Drive    string    `json:"drive,omitempty"`
	Message  string    `json:"message,omitempty"`
	Progress *Progress `json:"progress,omitempty"` // for rip progress events
}

// Progress represents rip progress information.
type Progress struct {
	Title      int     `json:"title"`
	Percent    float64 `json:"percent"`
	SpeedMBPS  float64 `json:"speed_mbps"`
	TotalBytes int64   `json:"total_bytes,omitempty"` // bytes written so far; lets the client estimate remaining time
	EstimatedTotal int64 `json:"estimated_total,omitempty"` // estimated total bytes to write; more accurate than total/pct
	Phase      string  `json:"phase,omitempty"`
	TitleIndex int     `json:"title_index,omitempty"` // N3: MSG:5017 index of the last saved title
	TitleTotal int     `json:"title_total,omitempty"` // N3: total titles in this rip
	RipPhase   string  `json:"rip_phase,omitempty"`   // "scanning" | "saving" | "complete"
}

// Subscriber represents an SSE client connection.
type Subscriber struct {
	ch     chan Event
	closed atomic.Bool
}

// Closed returns true if the subscriber has been closed.
func (s *Subscriber) Closed() bool {
	return s.closed.Load()
}

// Chan returns the event channel for reading.
func (s *Subscriber) Chan() <-chan Event {
	return s.ch
}

// Manager handles SSE event broadcasting.
type Manager struct {
	mu          sync.RWMutex
	subscribers map[*Subscriber]struct{}
}

// New creates a new Manager.
func New() *Manager {
	return &Manager{
		subscribers: make(map[*Subscriber]struct{}),
	}
}

// Subscribe adds a new subscriber and returns it.
func (m *Manager) Subscribe() *Subscriber {
	sub := &Subscriber{
		ch: make(chan Event, 64),
	}

	m.mu.Lock()
	m.subscribers[sub] = struct{}{}
	totalSubs := len(m.subscribers)
	m.mu.Unlock()

	logger.Debug("SSE subscriber connected", "total", totalSubs)
	return sub
}

// Unsubscribe removes a subscriber. Idempotent.
func (m *Manager) Unsubscribe(sub *Subscriber) {
	if !sub.closed.CompareAndSwap(false, true) {
		return
	}
	m.mu.Lock()
	delete(m.subscribers, sub)
	totalSubs := len(m.subscribers)
	m.mu.Unlock()
	close(sub.ch)

	logger.Debug("SSE subscriber disconnected", "total", totalSubs)
}

// CloseAll closes all subscribers and clears the subscriber map.
func (m *Manager) CloseAll() {
	m.mu.Lock()
	for sub := range m.subscribers {
		if sub.closed.CompareAndSwap(false, true) {
			close(sub.ch)
		}
	}
	m.subscribers = make(map[*Subscriber]struct{})
	m.mu.Unlock()
}

// Publish sends an event to all active subscribers.
func (m *Manager) Publish(event Event) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	for sub := range m.subscribers {
		if sub.Closed() {
			continue
		}
		select {
		case sub.ch <- event:
		default:
			logger.Debug("SSE drop: slow subscriber", "event", event.Type)
		}
	}
}
