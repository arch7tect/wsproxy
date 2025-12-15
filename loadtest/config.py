import os
from dataclasses import dataclass


@dataclass
class Config:
    redis_url: str
    fastapi_host: str
    fastapi_port: int
    fastapi_workers: int
    session_ttl_seconds: int
    token_delay_ms: int
    llm_response_text: str
    ws_max_reconnect_attempts: int
    ws_initial_backoff_seconds: float
    ws_max_backoff_seconds: float
    jwt_secret: str
    jwt_algorithm: str

    @classmethod
    def from_env(cls) -> "Config":
        jwt_secret = os.getenv("JWT_SECRET", "")
        if not jwt_secret or len(jwt_secret) < 32:
            raise ValueError("JWT_SECRET required and must be >=32 bytes")

        return cls(
            redis_url=os.getenv("REDIS_URL", "redis://127.0.0.1:6379"),
            fastapi_host=os.getenv("FASTAPI_HOST", "0.0.0.0"),
            fastapi_port=int(os.getenv("FASTAPI_PORT", "8000")),
            fastapi_workers=int(os.getenv("FASTAPI_WORKERS", "4")),
            session_ttl_seconds=int(os.getenv("SESSION_TTL_SECONDS", "3600")),
            token_delay_ms=int(os.getenv("TOKEN_DELAY_MS", "50")),
            llm_response_text=os.getenv(
                "LLM_RESPONSE_TEXT",
                "This is a simulated LLM response with multiple tokens streaming back to the client.",
            ),
            ws_max_reconnect_attempts=int(os.getenv("WS_MAX_RECONNECT_ATTEMPTS", "5")),
            ws_initial_backoff_seconds=float(os.getenv("WS_INITIAL_BACKOFF_SECONDS", "1.0")),
            ws_max_backoff_seconds=float(os.getenv("WS_MAX_BACKOFF_SECONDS", "30.0")),
            jwt_secret=jwt_secret,
            jwt_algorithm=os.getenv("JWT_ALGORITHM", "HS256"),
        )


config = Config.from_env()
