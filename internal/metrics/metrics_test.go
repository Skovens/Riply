package metrics

import (
	"encoding/json"
	"testing"
)

// The /api/metrics endpoint and progress reporting read exactly these keys;
// a renamed key silently breaks the dashboard.
func TestSnapshotTracksCounters(t *testing.T) {
	var m Metrics
	m.TotalRips.Add(3)
	m.SuccessfulRips.Add(2)
	m.FailedRips.Add(1)
	m.ActiveRips.Add(1)

	snap := m.Snapshot()
	want := map[string]int64{
		"total_rips":      3,
		"successful_rips": 2,
		"failed_rips":     1,
		"active_rips":     1,
	}
	for k, v := range want {
		if snap[k] != v {
			t.Errorf("Snapshot()[%q] = %d, want %d", k, snap[k], v)
		}
	}
}

func TestToJSONMarshalsSnapshot(t *testing.T) {
	var m Metrics
	m.TotalRips.Store(7)
	data := m.ToJSON()
	var got map[string]int64
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("ToJSON not valid JSON: %v (%s)", err, data)
	}
	if got["total_rips"] != 7 {
		t.Errorf("total_rips = %d, want 7", got["total_rips"])
	}
}
