import time
from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class SessionMetrics:
    session_id: str
    create_time: float = 0.0
    connect_time: float = 0.0
    query_latencies: List[float] = field(default_factory=list)
    token_counts: List[int] = field(default_factory=list)
    close_time: float = 0.0
    error: Optional[str] = None
    total_tokens: int = 0
    reconnect_attempts: int = 0
    successful_reconnects: int = 0

    def add_query_result(self, latency: float, token_count: int):
        self.query_latencies.append(latency)
        self.token_counts.append(token_count)
        self.total_tokens += token_count

    def mark_error(self, error: str):
        self.error = error

    def record_reconnect_attempt(self):
        self.reconnect_attempts += 1

    def record_successful_reconnect(self):
        self.successful_reconnects += 1

    def is_success(self) -> bool:
        return self.error is None


@dataclass
class AggregatedMetrics:
    total_sessions: int
    successful_sessions: int
    failed_sessions: int
    total_queries: int
    total_tokens: int
    avg_create_time: float
    avg_connect_time: float
    avg_query_latency: float
    p50_query_latency: float
    p95_query_latency: float
    p99_query_latency: float
    min_query_latency: float
    max_query_latency: float
    queries_per_second: float
    tokens_per_second: float
    success_rate: float
    test_duration: float
    total_reconnect_attempts: int
    total_successful_reconnects: int
    sessions_with_reconnects: int

    @classmethod
    def from_session_metrics(cls, sessions: List[SessionMetrics], test_duration: float) -> "AggregatedMetrics":
        successful = [s for s in sessions if s.is_success()]
        failed = [s for s in sessions if not s.is_success()]

        all_latencies = []
        total_tokens = 0
        total_queries = 0

        for session in successful:
            all_latencies.extend(session.query_latencies)
            total_tokens += session.total_tokens
            total_queries += len(session.query_latencies)

        all_latencies.sort()

        if all_latencies:
            p50_idx = int(len(all_latencies) * 0.50)
            p95_idx = int(len(all_latencies) * 0.95)
            p99_idx = int(len(all_latencies) * 0.99)

            p50 = all_latencies[p50_idx]
            p95 = all_latencies[p95_idx]
            p99 = all_latencies[p99_idx]
            min_lat = all_latencies[0]
            max_lat = all_latencies[-1]
            avg_lat = sum(all_latencies) / len(all_latencies)
        else:
            p50 = p95 = p99 = min_lat = max_lat = avg_lat = 0.0

        avg_create = sum(s.create_time for s in successful) / len(successful) if successful else 0.0
        avg_connect = sum(s.connect_time for s in successful) / len(successful) if successful else 0.0

        qps = total_queries / test_duration if test_duration > 0 else 0.0
        tps = total_tokens / test_duration if test_duration > 0 else 0.0
        success_rate = len(successful) / len(sessions) * 100 if sessions else 0.0

        total_reconnect_attempts = sum(s.reconnect_attempts for s in sessions)
        total_successful_reconnects = sum(s.successful_reconnects for s in sessions)
        sessions_with_reconnects = sum(1 for s in sessions if s.reconnect_attempts > 0)

        return cls(
            total_sessions=len(sessions),
            successful_sessions=len(successful),
            failed_sessions=len(failed),
            total_queries=total_queries,
            total_tokens=total_tokens,
            avg_create_time=avg_create,
            avg_connect_time=avg_connect,
            avg_query_latency=avg_lat,
            p50_query_latency=p50,
            p95_query_latency=p95,
            p99_query_latency=p99,
            min_query_latency=min_lat,
            max_query_latency=max_lat,
            queries_per_second=qps,
            tokens_per_second=tps,
            success_rate=success_rate,
            test_duration=test_duration,
            total_reconnect_attempts=total_reconnect_attempts,
            total_successful_reconnects=total_successful_reconnects,
            sessions_with_reconnects=sessions_with_reconnects,
        )
