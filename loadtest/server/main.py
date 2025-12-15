import asyncio
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, BackgroundTasks
from redis.asyncio import Redis, ConnectionPool

from loadtest.config import config
from loadtest.server.models import (
    SessionCreateResponse,
    SessionCloseResponse,
    RunRequest,
    RunResponse,
    HealthResponse,
)
from loadtest.server.session_manager import SessionManager
from loadtest.server.redis_client import RedisPublisher
from loadtest.server.llm_simulator import LLMSimulator


class AppState:
    redis_pool: ConnectionPool
    redis: Redis
    session_manager: SessionManager
    redis_publisher: RedisPublisher
    llm_simulator: LLMSimulator


@asynccontextmanager
async def lifespan(app: FastAPI):
    app.state.redis_pool = ConnectionPool.from_url(config.redis_url)
    app.state.redis = Redis(connection_pool=app.state.redis_pool)
    app.state.session_manager = SessionManager(app.state.redis)
    app.state.redis_publisher = RedisPublisher()
    await app.state.redis_publisher.connect()
    app.state.llm_simulator = LLMSimulator(app.state.redis_publisher)

    yield

    await app.state.redis.aclose()
    await app.state.redis_publisher.close()
    await app.state.redis_pool.aclose()


app = FastAPI(title="Load Test Server", lifespan=lifespan)


@app.get("/health", response_model=HealthResponse)
async def health():
    return HealthResponse(status="healthy")


@app.post("/session/create", response_model=SessionCreateResponse)
async def create_session():
    session_id, auth_token = await app.state.session_manager.create_session()
    return SessionCreateResponse(session_id=session_id, auth_token=auth_token)


@app.post("/session/{session_id}/close", response_model=SessionCloseResponse)
async def close_session(session_id: str):
    exists = await app.state.session_manager.session_exists(session_id)
    if not exists:
        raise HTTPException(status_code=404, detail="Session not found")

    await app.state.session_manager.close_session(session_id)
    return SessionCloseResponse(status="closed")


@app.post("/session/{session_id}/run", response_model=RunResponse)
async def run_query(session_id: str, request: RunRequest, background_tasks: BackgroundTasks):
    exists = await app.state.session_manager.session_exists(session_id)
    if not exists:
        raise HTTPException(status_code=404, detail="Session not found")

    background_tasks.add_task(
        app.state.llm_simulator.stream_response,
        session_id,
        request.query,
    )

    return RunResponse(status="streaming")
