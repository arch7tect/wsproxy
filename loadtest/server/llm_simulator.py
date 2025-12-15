import asyncio

from loadtest.config import config
from loadtest.server.redis_client import RedisPublisher


class LLMSimulator:
    def __init__(self, redis_publisher: RedisPublisher):
        self.redis_publisher = redis_publisher
        self.token_delay = config.token_delay_ms / 1000.0
        self.response_text = config.llm_response_text

    async def stream_response(self, session_id: str, query: str):
        tokens = self.response_text.split()

        for token in tokens:
            await self.redis_publisher.publish_token(session_id, token)
            await asyncio.sleep(self.token_delay)

        await self.redis_publisher.publish_done(session_id)
