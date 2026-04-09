/**
 * Event processing pipeline.
 */

class EventBus {
  constructor() {
    this.handlers = new Map();
    this.eventCount = 0;
  }

  on(eventName, handler) {
    if (!this.handlers.has(eventName)) {
      this.handlers.set(eventName, []);
    }
    this.handlers.get(eventName).push(handler);
  }

  emit(eventName, payload) {
    this.eventCount++;
    const handlers = this.handlers.get(eventName) || [];
    for (const handler of handlers) {
      handler(payload);
    }
  }

  off(eventName, handler) {
    const handlers = this.handlers.get(eventName);
    if (handlers) {
      const idx = handlers.indexOf(handler);
      if (idx !== -1) {
        handlers.splice(idx, 1);
      }
    }
  }
}

function computeStats(values) {
  if (values.length === 0) {
    return { mean: 0, median: 0, stdDev: 0, min: 0, max: 0 };
  }

  const sorted = [...values].sort((a, b) => a - b);
  const sum = sorted.reduce((acc, v) => acc + v, 0);
  const mean = sum / sorted.length;
  const median = sorted[Math.floor(sorted.length / 2)];
  const variance = sorted.reduce((acc, v) => acc + (v - mean) ** 2, 0) / sorted.length;

  return {
    mean,
    median,
    stdDev: Math.sqrt(variance),
    min: sorted[0],
    max: sorted[sorted.length - 1],
  };
}

async function fetchAndProcess(url) {
  try {
    const response = await fetch(url);
    const data = await response.json();
    return data.map((item) => ({
      id: item.id,
      score: item.value * 0.75,
      timestamp: new Date(item.ts).toISOString(),
    }));
  } catch (error) {
    console.error(`Failed to fetch ${url}:`, error.message);
    return [];
  }
}

module.exports = { EventBus, computeStats, fetchAndProcess };
