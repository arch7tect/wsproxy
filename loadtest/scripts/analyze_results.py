#!/usr/bin/env python3

import csv
import sys
from pathlib import Path
from collections import Counter


def analyze_csv(csv_path: Path):
    if not csv_path.exists():
        print(f"Error: File not found: {csv_path}")
        sys.exit(1)

    latencies = []
    tokens = []
    errors = Counter()
    sessions = set()

    with open(csv_path, "r") as f:
        reader = csv.DictReader(f)
        for row in reader:
            sessions.add(row["session_id"])
            if row["error"]:
                errors[row["error"]] += 1
            else:
                latencies.append(float(row["latency"]))
                tokens.append(int(row["tokens"]))

    if not latencies:
        print("No successful queries found in results")
        return

    latencies.sort()
    total_queries = len(latencies)

    print("\n" + "=" * 80)
    print("DETAILED ANALYSIS")
    print("=" * 80)
    print(f"\nSessions: {len(sessions)}")
    print(f"Total Queries: {total_queries}")
    print(f"Total Tokens: {sum(tokens)}")

    if errors:
        print(f"\nErrors:")
        for error, count in errors.most_common():
            print(f"  {error}: {count}")

    print(f"\nLatency Distribution (seconds):")
    percentiles = [10, 25, 50, 75, 90, 95, 99]
    for p in percentiles:
        idx = int(len(latencies) * p / 100)
        print(f"  p{p:2d}: {latencies[idx]:.4f}")

    print(f"\nLatency Histogram:")
    bins = [0.0, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, float('inf')]
    bin_labels = ["<100ms", "100-200ms", "200-500ms", "0.5-1s", "1-2s", "2-5s", ">5s"]

    histogram = [0] * len(bin_labels)
    for lat in latencies:
        for i, threshold in enumerate(bins[1:]):
            if lat < threshold:
                histogram[i] += 1
                break

    max_count = max(histogram)
    bar_width = 50

    for label, count in zip(bin_labels, histogram):
        bar = "#" * int(count / max_count * bar_width) if max_count > 0 else ""
        percentage = count / total_queries * 100
        print(f"  {label:10s} {bar:50s} {count:5d} ({percentage:5.1f}%)")

    print("=" * 80 + "\n")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python analyze_results.py <path_to_csv>")
        sys.exit(1)

    analyze_csv(Path(sys.argv[1]))
