import asyncio
import logging
import time
from typing import Optional

import aiohttp
import orjson
import websockets
from websockets.exceptions import WebSocketException

from loadtest.client.metrics import SessionMetrics
from loadtest.config import config

logger = logging.getLogger(__name__)


class SessionClient:
    def __init__(self, fastapi_url: str, wsproxy_url: str, num_queries: int, agent_id: str = "load-test-agent"):
        self.fastapi_url = fastapi_url
        self.wsproxy_url = wsproxy_url
        self.num_queries = num_queries
        self.agent_id = agent_id
        self.session_id: Optional[str] = None
        self.auth_token: Optional[str] = None
        self.metrics: Optional[SessionMetrics] = None

    async def run(self) -> SessionMetrics:
        try:
            await self._create_session()
            await self._connect_websocket()
            await self._run_queries()
            await self._close_session()
        except Exception as e:
            if self.metrics:
                self.metrics.mark_error(str(e))
            else:
                self.metrics = SessionMetrics(session_id="unknown", error=str(e))

        return self.metrics

    async def _create_session(self):
        start = time.time()
        async with aiohttp.ClientSession() as session:
            async with session.post(f"{self.fastapi_url}/session/create") as resp:
                resp.raise_for_status()
                data = await resp.json()
                self.session_id = data["session_id"]
                self.auth_token = data["auth_token"]
                self.metrics = SessionMetrics(session_id=self.session_id)
                self.metrics.create_time = time.time() - start

    async def _connect_websocket_with_retry(self):
        ws_url = f"{self.wsproxy_url}/{self.agent_id}/ws/{self.session_id}"
        headers = {"Authorization": f"Bearer {self.auth_token}"}

        backoff = config.ws_initial_backoff_seconds
        for attempt in range(config.ws_max_reconnect_attempts + 1):
            try:
                websocket = await websockets.connect(ws_url, additional_headers=headers)
                if attempt > 0:
                    self.metrics.record_successful_reconnect()
                    logger.info(f"Reconnected after {attempt} attempts for session {self.session_id}")
                return websocket
            except (WebSocketException, OSError, asyncio.TimeoutError) as e:
                if attempt < config.ws_max_reconnect_attempts:
                    self.metrics.record_reconnect_attempt()
                    logger.warning(
                        f"WebSocket connection failed for session {self.session_id} "
                        f"(attempt {attempt + 1}/{config.ws_max_reconnect_attempts + 1}): {e}. "
                        f"Retrying in {backoff:.1f}s..."
                    )
                    await asyncio.sleep(backoff)
                    backoff = min(backoff * 2, config.ws_max_backoff_seconds)
                else:
                    logger.error(
                        f"WebSocket connection failed for session {self.session_id} "
                        f"after {config.ws_max_reconnect_attempts + 1} attempts: {e}"
                    )
                    raise

    async def _connect_websocket(self):
        start = time.time()
        websocket = await self._connect_websocket_with_retry()
        self.metrics.connect_time = time.time() - start
        await websocket.close()

    async def _run_queries(self):
        for i in range(self.num_queries):
            await self._run_single_query(f"Query {i + 1}")

    async def _run_single_query(self, query: str):
        start = time.time()
        token_count = 0

        async with aiohttp.ClientSession() as http_session:
            async with http_session.post(
                f"{self.fastapi_url}/session/{self.session_id}/run",
                json={"query": query},
            ) as resp:
                resp.raise_for_status()

        websocket = None
        try:
            websocket = await self._connect_websocket_with_retry()

            while True:
                try:
                    message = await asyncio.wait_for(websocket.recv(), timeout=10.0)
                    data = orjson.loads(message)

                    if data["type"] == "token":
                        token_count += 1
                    elif data["type"] == "done":
                        break
                except asyncio.TimeoutError:
                    break
                except (WebSocketException, OSError) as e:
                    logger.warning(
                        f"Connection lost during query for session {self.session_id}: {e}. "
                        f"Attempting reconnection..."
                    )
                    if websocket:
                        try:
                            await websocket.close()
                        except Exception:
                            pass

                    websocket = await self._connect_websocket_with_retry()

        finally:
            if websocket:
                try:
                    await websocket.close()
                except Exception:
                    pass

        latency = time.time() - start
        self.metrics.add_query_result(latency, token_count)

    async def _close_session(self):
        start = time.time()
        async with aiohttp.ClientSession() as session:
            async with session.post(
                f"{self.fastapi_url}/session/{self.session_id}/close"
            ) as resp:
                resp.raise_for_status()
                self.metrics.close_time = time.time() - start
