import orjson
from redis.asyncio import Redis, ConnectionPool

from loadtest.config import config


class RedisPublisher:
    def __init__(self):
        self.pool = ConnectionPool.from_url(config.redis_url, decode_responses=False)
        self.redis: Optional[Redis] = None

    async def connect(self):
        self.redis = Redis(connection_pool=self.pool)

    async def close(self):
        if self.redis:
            await self.redis.aclose()
        await self.pool.aclose()

    async def publish_downstream(self, session_id: str, message_type: str, content: str):
        channel = f"session:{session_id}:down"
        message = orjson.dumps({"type": message_type, "content": content})
        await self.redis.publish(channel, message)

    async def publish_token(self, session_id: str, token: str):
        await self.publish_downstream(session_id, "token", token)

    async def publish_done(self, session_id: str):
        await self.publish_downstream(session_id, "done", "")


from typing import Optional
