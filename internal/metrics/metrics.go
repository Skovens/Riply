package metrics

import (
	"encoding/json"
	"sync/atomic"
)

// Metrics holds application metrics.
type Metrics struct {
	TotalRips      atomic.Int64
	SuccessfulRips atomic.Int64
	FailedRips     atomic.Int64
	ActiveRips     atomic.Int64
}

// Snapshot returns a JSON-serializable snapshot of current metrics.
func (m *Metrics) Snapshot() map[string]int64 {
	return map[string]int64{
		"total_rips":      m.TotalRips.Load(),
		"successful_rips": m.SuccessfulRips.Load(),
		"failed_rips":     m.FailedRips.Load(),
		"active_rips":     m.ActiveRips.Load(),
	}
}

// ToJSON returns the metrics as JSON bytes.
func (m *Metrics) ToJSON() []byte {
	data, err := json.Marshal(m.Snapshot())
	if err != nil {
		return []byte("{}")
	}
	return data
}
