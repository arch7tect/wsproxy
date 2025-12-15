import asyncio
import os
import time
from pathlib import Path
from typing import List

from loadtest.client.session_client import SessionClient
from loadtest.client.metrics import SessionMetrics, AggregatedMetrics
from loadtest.client.reporter import Reporter


class LoadTestClient:
    def __init__(
        self,
        fastapi_url: str,
        wsproxy_url: str,
        num_clients: int,
        queries_per_client: int,
        output_dir: Path,
    ):
        self.fastapi_url = fastapi_url
        self.wsproxy_url = wsproxy_url
        self.num_clients = num_clients
        self.queries_per_client = queries_per_client
        self.output_dir = output_dir
        self.output_dir.mkdir(parents=True, exist_ok=True)

    async def run(self) -> AggregatedMetrics:
        print(f"\nStarting load test:")
        print(f"  Clients: {self.num_clients}")
        print(f"  Queries per client: {self.queries_per_client}")
        print(f"  FastAPI URL: {self.fastapi_url}")
        print(f"  wsproxy URL: {self.wsproxy_url}")
        print(f"\nLaunching clients...")

        start_time = time.time()

        tasks = [
            self._run_session_client()
            for _ in range(self.num_clients)
        ]

        session_metrics: List[SessionMetrics] = await asyncio.gather(*tasks)

        test_duration = time.time() - start_time

        aggregated = AggregatedMetrics.from_session_metrics(session_metrics, test_duration)

        Reporter.print_summary(aggregated)

        timestamp = time.strftime("%Y%m%d_%H%M%S")
        Reporter.export_csv(
            session_metrics,
            self.output_dir / f"loadtest_{timestamp}.csv",
        )
        Reporter.export_json(
            aggregated,
            self.output_dir / f"loadtest_{timestamp}_summary.json",
        )

        return aggregated

    async def _run_session_client(self) -> SessionMetrics:
        client = SessionClient(
            self.fastapi_url,
            self.wsproxy_url,
            self.queries_per_client,
        )
        return await client.run()


async def main():
    fastapi_url = os.getenv("FASTAPI_URL", "http://localhost:8000")
    wsproxy_url = os.getenv("WSPROXY_URL", "ws://localhost:4040")
    num_clients = int(os.getenv("CLIENTS", "10"))
    queries_per_client = int(os.getenv("QUERIES_PER_CLIENT", "5"))
    output_dir = Path(os.getenv("OUTPUT_DIR", "./loadtest_results"))

    client = LoadTestClient(
        fastapi_url=fastapi_url,
        wsproxy_url=wsproxy_url,
        num_clients=num_clients,
        queries_per_client=queries_per_client,
        output_dir=output_dir,
    )

    await client.run()


if __name__ == "__main__":
    asyncio.run(main())
