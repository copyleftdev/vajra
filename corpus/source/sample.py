"""A simple data processor with classes and functions."""

from typing import List, Optional, Dict
from dataclasses import dataclass


@dataclass
class Record:
    """A single data record."""
    record_id: str
    timestamp: str
    values: Dict[str, float]
    tags: List[str]
    parent_id: Optional[str] = None


class DataProcessor:
    """Processes batches of records."""

    MAX_BATCH_SIZE = 1000
    DEFAULT_THRESHOLD = 0.95

    def __init__(self, threshold: float = DEFAULT_THRESHOLD):
        self.threshold = threshold
        self._records: List[Record] = []
        self._processed_count = 0

    def add_record(self, record: Record) -> bool:
        """Add a record to the batch."""
        if len(self._records) >= self.MAX_BATCH_SIZE:
            return False
        self._records.append(record)
        return True

    def process_batch(self) -> List[Dict[str, float]]:
        """Process all records and return aggregated results."""
        results = []
        for record in self._records:
            summary = self._analyze_record(record)
            if summary["score"] >= self.threshold:
                results.append(summary)
            self._processed_count += 1
        self._records.clear()
        return results

    def _analyze_record(self, record: Record) -> Dict[str, float]:
        """Compute summary statistics for a single record."""
        values = list(record.values.values())
        if not values:
            return {"score": 0.0, "mean": 0.0, "count": 0.0}

        mean_val = sum(values) / len(values)
        max_val = max(values)
        min_val = min(values)

        return {
            "score": mean_val / max_val if max_val > 0 else 0.0,
            "mean": mean_val,
            "max": max_val,
            "min": min_val,
            "count": float(len(values)),
        }

    @property
    def processed_count(self) -> int:
        return self._processed_count

    @property
    def pending_count(self) -> int:
        return len(self._records)


def find_anomalies(records: List[Record], z_threshold: float = 3.0) -> List[str]:
    """Find records with anomalous values."""
    anomalies = []
    all_values = []

    for record in records:
        all_values.extend(record.values.values())

    if len(all_values) < 2:
        return anomalies

    mean = sum(all_values) / len(all_values)
    variance = sum((v - mean) ** 2 for v in all_values) / len(all_values)
    std_dev = variance ** 0.5

    if std_dev == 0:
        return anomalies

    for record in records:
        for key, value in record.values.items():
            z_score = abs(value - mean) / std_dev
            if z_score > z_threshold:
                anomalies.append(f"{record.record_id}.{key}: z={z_score:.2f}")

    return anomalies
