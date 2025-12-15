import csv
import orjson
from pathlib import Path
from typing import List

from loadtest.client.metrics import SessionMetrics, AggregatedMetrics


class Reporter:
    @staticmethod
    def print_summary(metrics: AggregatedMetrics):
        print("\n" + "=" * 80)
        print("LOAD TEST RESULTS")
        print("=" * 80)
        print(f"\nTest Duration: {metrics.test_duration:.2f}s")
        print(f"\nSessions:")
        print(f"  Total:      {metrics.total_sessions}")
        print(f"  Successful: {metrics.successful_sessions}")
        print(f"  Failed:     {metrics.failed_sessions}")
        print(f"  Success Rate: {metrics.success_rate:.2f}%")
        print(f"\nQueries:")
        print(f"  Total: {metrics.total_queries}")
        print(f"  QPS:   {metrics.queries_per_second:.2f}")
        print(f"\nTokens:")
        print(f"  Total: {metrics.total_tokens}")
        print(f"  TPS:   {metrics.tokens_per_second:.2f}")
        print(f"\nLatency (seconds):")
        print(f"  Create Session:  {metrics.avg_create_time:.4f}")
        print(f"  Connect WS:      {metrics.avg_connect_time:.4f}")
        print(f"  Query (avg):     {metrics.avg_query_latency:.4f}")
        print(f"  Query (p50):     {metrics.p50_query_latency:.4f}")
        print(f"  Query (p95):     {metrics.p95_query_latency:.4f}")
        print(f"  Query (p99):     {metrics.p99_query_latency:.4f}")
        print(f"  Query (min):     {metrics.min_query_latency:.4f}")
        print(f"  Query (max):     {metrics.max_query_latency:.4f}")
        print(f"\nReconnections:")
        print(f"  Total Attempts:      {metrics.total_reconnect_attempts}")
        print(f"  Successful:          {metrics.total_successful_reconnects}")
        print(f"  Sessions Affected:   {metrics.sessions_with_reconnects}")
        print("=" * 80 + "\n")

    @staticmethod
    def export_csv(sessions: List[SessionMetrics], output_path: Path):
        with open(output_path, "w", newline="") as csvfile:
            writer = csv.writer(csvfile)
            writer.writerow([
                "session_id",
                "query_num",
                "latency",
                "tokens",
                "create_time",
                "connect_time",
                "close_time",
                "reconnect_attempts",
                "successful_reconnects",
                "error",
            ])

            for session in sessions:
                for i, (latency, tokens) in enumerate(
                    zip(session.query_latencies, session.token_counts), 1
                ):
                    writer.writerow([
                        session.session_id,
                        i,
                        f"{latency:.4f}",
                        tokens,
                        f"{session.create_time:.4f}",
                        f"{session.connect_time:.4f}",
                        f"{session.close_time:.4f}",
                        session.reconnect_attempts,
                        session.successful_reconnects,
                        session.error or "",
                    ])

        print(f"Detailed results exported to: {output_path}")

    @staticmethod
    def export_json(metrics: AggregatedMetrics, output_path: Path):
        data = {
            "total_sessions": metrics.total_sessions,
            "successful_sessions": metrics.successful_sessions,
            "failed_sessions": metrics.failed_sessions,
            "success_rate": metrics.success_rate,
            "total_queries": metrics.total_queries,
            "total_tokens": metrics.total_tokens,
            "queries_per_second": metrics.queries_per_second,
            "tokens_per_second": metrics.tokens_per_second,
            "avg_create_time": metrics.avg_create_time,
            "avg_connect_time": metrics.avg_connect_time,
            "avg_query_latency": metrics.avg_query_latency,
            "p50_query_latency": metrics.p50_query_latency,
            "p95_query_latency": metrics.p95_query_latency,
            "p99_query_latency": metrics.p99_query_latency,
            "min_query_latency": metrics.min_query_latency,
            "max_query_latency": metrics.max_query_latency,
            "test_duration": metrics.test_duration,
            "total_reconnect_attempts": metrics.total_reconnect_attempts,
            "total_successful_reconnects": metrics.total_successful_reconnects,
            "sessions_with_reconnects": metrics.sessions_with_reconnects,
        }

        with open(output_path, "wb") as f:
            f.write(orjson.dumps(data, option=orjson.OPT_INDENT_2))

        print(f"Summary exported to: {output_path}")
