package main

import (
	"fmt"
	"math"
	"sort"
	"strings"
)

// MetricPoint represents a single metric observation.
type MetricPoint struct {
	Name      string
	Value     float64
	Timestamp int64
	Tags      map[string]string
}

// MetricSummary holds aggregated statistics for a metric.
type MetricSummary struct {
	Name    string
	Count   int
	Mean    float64
	Median  float64
	StdDev  float64
	Min     float64
	Max     float64
	P95     float64
}

// Aggregator collects metric points and computes summaries.
type Aggregator struct {
	points map[string][]float64
}

// NewAggregator creates a new Aggregator.
func NewAggregator() *Aggregator {
	return &Aggregator{
		points: make(map[string][]float64),
	}
}

// Add records a metric point.
func (a *Aggregator) Add(point MetricPoint) {
	a.points[point.Name] = append(a.points[point.Name], point.Value)
}

// Summarize computes summaries for all collected metrics.
func (a *Aggregator) Summarize() []MetricSummary {
	summaries := make([]MetricSummary, 0, len(a.points))
	for name, values := range a.points {
		if len(values) == 0 {
			continue
		}
		summaries = append(summaries, computeSummary(name, values))
	}
	sort.Slice(summaries, func(i, j int) bool {
		return summaries[i].Name < summaries[j].Name
	})
	return summaries
}

func computeSummary(name string, values []float64) MetricSummary {
	sorted := make([]float64, len(values))
	copy(sorted, values)
	sort.Float64s(sorted)

	n := float64(len(sorted))
	sum := 0.0
	for _, v := range sorted {
		sum += v
	}
	mean := sum / n

	variance := 0.0
	for _, v := range sorted {
		variance += (v - mean) * (v - mean)
	}
	variance /= n

	median := sorted[len(sorted)/2]
	p95Idx := int(math.Ceil(0.95*n)) - 1
	if p95Idx >= len(sorted) {
		p95Idx = len(sorted) - 1
	}

	return MetricSummary{
		Name:   name,
		Count:  len(sorted),
		Mean:   mean,
		Median: median,
		StdDev: math.Sqrt(variance),
		Min:    sorted[0],
		Max:    sorted[len(sorted)-1],
		P95:    sorted[p95Idx],
	}
}

func main() {
	agg := NewAggregator()
	agg.Add(MetricPoint{Name: "latency_ms", Value: 12.5, Timestamp: 1000, Tags: map[string]string{"service": "api"}})
	agg.Add(MetricPoint{Name: "latency_ms", Value: 15.2, Timestamp: 1001, Tags: map[string]string{"service": "api"}})
	agg.Add(MetricPoint{Name: "latency_ms", Value: 250.0, Timestamp: 1002, Tags: map[string]string{"service": "api"}})
	agg.Add(MetricPoint{Name: "cpu_pct", Value: 45.0, Timestamp: 1000, Tags: map[string]string{"host": "web-01"}})
	agg.Add(MetricPoint{Name: "cpu_pct", Value: 52.3, Timestamp: 1001, Tags: map[string]string{"host": "web-01"}})

	for _, s := range agg.Summarize() {
		fmt.Printf("%s: mean=%.2f median=%.2f stddev=%.2f p95=%.2f [%d points]\n",
			s.Name, s.Mean, s.Median, s.StdDev, s.P95, s.Count)
	}

	_ = strings.NewReader("unused but imported")
}
