import jwt
import time
import uuid
from typing import Optional

from redis.asyncio import Redis

from loadtest.config import config


class SessionManager:
    def __init__(self, redis_client: Redis):
        self.redis = redis_client
        self.ttl = config.session_ttl_seconds
        self.jwt_secret = config.jwt_secret
        self.jwt_algorithm = config.jwt_algorithm

    async def create_session(self) -> tuple[str, str]:
        session_id = str(uuid.uuid4())
        auth_token = self._create_jwt(session_id)

        meta_key = f"session:{session_id}:meta"
        await self.redis.hset(
            meta_key,
            mapping={"status": "active", "created_at": str(int(time.time()))},
        )
        await self.redis.expire(meta_key, self.ttl)

        return session_id, auth_token

    def _create_jwt(self, session_id: str) -> str:
        payload = {
            "session_id": session_id,
            "iat": int(time.time()),
        }
        return jwt.encode(payload, self.jwt_secret, algorithm=self.jwt_algorithm)

    async def close_session(self, session_id: str) -> bool:
        meta_key = f"session:{session_id}:meta"
        await self.redis.hset(meta_key, "status", "closed")
        return True

    async def get_session_status(self, session_id: str) -> Optional[str]:
        meta_key = f"session:{session_id}:meta"
        status = await self.redis.hget(meta_key, "status")
        return status.decode() if status else None

    async def session_exists(self, session_id: str) -> bool:
        meta_key = f"session:{session_id}:meta"
        return await self.redis.exists(meta_key) > 0
