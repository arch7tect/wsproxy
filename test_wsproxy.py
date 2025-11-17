#!/usr/bin/env python3
"""
Integration test for wsproxy WebSocket server.
Tests authentication, bidirectional messaging, and Redis pub/sub integration.
"""
import asyncio
import json
import sys
from typing import Optional

import aiohttp
import redis.asyncio as aioredis
import websockets


class WsProxyTester:
    def __init__(
        self,
        ws_url: str = "ws://localhost:4040",
        redis_url: str = "redis://localhost:6379",
        health_url: str = "http://localhost:4040/health",
    ):
        self.ws_url = ws_url
        self.redis_url = redis_url
        self.health_url = health_url
        self.redis: Optional[aioredis.Redis] = None

    async def setup(self):
        """Initialize Redis connection."""
        self.redis = await aioredis.from_url(self.redis_url, decode_responses=True)
        print("✓ Connected to Redis")

    async def cleanup(self):
        """Close Redis connection."""
        if self.redis:
            await self.redis.close()
            print("✓ Redis connection closed")

    async def check_health(self) -> bool:
        """Check if wsproxy is healthy."""
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(self.health_url) as resp:
                    if resp.status == 200:
                        data = await resp.json()
                        print(f"✓ Health check passed: {data}")
                        return data.get("status") == "healthy"
                    return False
        except Exception as e:
            print(f"✗ Health check failed: {e}")
            return False

    async def set_auth_token(self, session_id: str, token: str, ttl: int = 60):
        """Set authentication token in Redis."""
        key = f"session:{session_id}:auth"
        await self.redis.setex(key, ttl, token)
        print(f"✓ Auth token set for session '{session_id}' (TTL: {ttl}s)")

    async def clear_auth_token(self, session_id: str):
        """Clear authentication token from Redis."""
        key = f"session:{session_id}:auth"
        await self.redis.delete(key)
        print(f"✓ Auth token cleared for session '{session_id}'")

    async def test_websocket_connection(
        self, agent_id: str, session_id: str, token: str
    ) -> bool:
        """Test basic WebSocket connection with authentication."""
        print(f"\n=== Testing WebSocket Connection ===")
        print(f"Agent: {agent_id}, Session: {session_id}")

        await self.set_auth_token(session_id, token)

        ws_endpoint = f"{self.ws_url}/{agent_id}/ws/{session_id}"
        headers = {"Authorization": f"Bearer {token}"}

        try:
            async with websockets.connect(ws_endpoint, additional_headers=headers) as ws:
                print(f"✓ WebSocket connected to {ws_endpoint}")

                await asyncio.sleep(0.5)
                print("✓ Connection stable")

                return True
        except Exception as e:
            print(f"✗ WebSocket connection failed: {e}")
            return False
        finally:
            await self.clear_auth_token(session_id)

    async def test_auth_failure(self, agent_id: str, session_id: str) -> bool:
        """Test that connection fails with invalid auth."""
        print(f"\n=== Testing Auth Failure ===")

        ws_endpoint = f"{self.ws_url}/{agent_id}/ws/{session_id}"
        headers = {"Authorization": "Bearer invalid_token"}

        try:
            async with websockets.connect(
                ws_endpoint, additional_headers=headers
            ) as ws:
                print(f"✗ Connection should have failed but succeeded")
                return False
        except Exception as e:
            error_msg = str(e)
            if "401" in error_msg or "403" in error_msg:
                print(f"✓ Auth correctly rejected: {error_msg}")
                return True
            print(f"✗ Unexpected error: {error_msg}")
            return False

    async def test_redis_to_websocket(
        self, agent_id: str, session_id: str, token: str
    ) -> bool:
        """Test Redis pub/sub to WebSocket forwarding."""
        print(f"\n=== Testing Redis → WebSocket ===")

        await self.set_auth_token(session_id, token)

        ws_endpoint = f"{self.ws_url}/{agent_id}/ws/{session_id}"
        headers = {"Authorization": f"Bearer {token}"}

        try:
            async with websockets.connect(ws_endpoint, additional_headers=headers) as ws:
                print("✓ WebSocket connected")

                test_message = {"type": "test", "data": "Hello from Redis"}
                channel = f"session:{session_id}:down"

                await asyncio.sleep(0.2)
                await self.redis.publish(channel, json.dumps(test_message))
                print(f"✓ Published to Redis channel '{channel}'")

                received = await asyncio.wait_for(ws.recv(), timeout=2.0)
                received_data = json.loads(received)

                if received_data == test_message:
                    print(f"✓ Received correct message: {received_data}")
                    return True
                else:
                    print(f"✗ Message mismatch. Expected: {test_message}, Got: {received_data}")
                    return False

        except asyncio.TimeoutError:
            print("✗ Timeout waiting for message from WebSocket")
            return False
        except Exception as e:
            print(f"✗ Test failed: {e}")
            return False
        finally:
            await self.clear_auth_token(session_id)

    async def test_websocket_to_redis(
        self, agent_id: str, session_id: str, token: str
    ) -> bool:
        """Test WebSocket to Redis pub/sub forwarding."""
        print(f"\n=== Testing WebSocket → Redis ===")

        await self.set_auth_token(session_id, token)

        ws_endpoint = f"{self.ws_url}/{agent_id}/ws/{session_id}"
        headers = {"Authorization": f"Bearer {token}"}

        channel = f"session:{session_id}:up"
        pubsub = self.redis.pubsub()

        try:
            await pubsub.subscribe(channel)
            print(f"✓ Subscribed to Redis channel '{channel}'")

            async with websockets.connect(ws_endpoint, additional_headers=headers) as ws:
                print("✓ WebSocket connected")

                await asyncio.sleep(0.2)

                test_message = {"type": "test", "data": "Hello from WebSocket"}
                await ws.send(json.dumps(test_message))
                print(f"✓ Sent message via WebSocket")

                async for message in pubsub.listen():
                    if message["type"] == "message":
                        received_data = json.loads(message["data"])
                        if received_data == test_message:
                            print(f"✓ Received correct message on Redis: {received_data}")
                            return True
                        else:
                            print(f"✗ Message mismatch. Expected: {test_message}, Got: {received_data}")
                            return False

                print("✗ No message received on Redis")
                return False

        except asyncio.TimeoutError:
            print("✗ Timeout waiting for message on Redis")
            return False
        except Exception as e:
            print(f"✗ Test failed: {e}")
            return False
        finally:
            await pubsub.unsubscribe(channel)
            await pubsub.close()
            await self.clear_auth_token(session_id)

    async def test_bidirectional(
        self, agent_id: str, session_id: str, token: str
    ) -> bool:
        """Test bidirectional messaging."""
        print(f"\n=== Testing Bidirectional Communication ===")

        await self.set_auth_token(session_id, token)

        ws_endpoint = f"{self.ws_url}/{agent_id}/ws/{session_id}"
        headers = {"Authorization": f"Bearer {token}"}

        channel_down = f"session:{session_id}:down"
        channel_up = f"session:{session_id}:up"
        pubsub = self.redis.pubsub()

        try:
            await pubsub.subscribe(channel_up)
            print(f"✓ Subscribed to Redis channel '{channel_up}'")

            async with websockets.connect(ws_endpoint, additional_headers=headers) as ws:
                print("✓ WebSocket connected")
                await asyncio.sleep(0.2)

                msg1 = {"type": "ping", "id": 1}
                await ws.send(json.dumps(msg1))
                print(f"✓ Sent WS→Redis: {msg1}")

                msg_received = False
                async for message in pubsub.listen():
                    if message["type"] == "message":
                        data = json.loads(message["data"])
                        if data == msg1:
                            print(f"✓ Received on Redis: {data}")
                            msg_received = True
                            break

                if not msg_received:
                    return False

                msg2 = {"type": "pong", "id": 2}
                await self.redis.publish(channel_down, json.dumps(msg2))
                print(f"✓ Published Redis→WS: {msg2}")

                received = await asyncio.wait_for(ws.recv(), timeout=2.0)
                data = json.loads(received)
                if data == msg2:
                    print(f"✓ Received on WebSocket: {data}")
                    return True
                else:
                    print(f"✗ Message mismatch")
                    return False

        except Exception as e:
            print(f"✗ Test failed: {e}")
            return False
        finally:
            await pubsub.unsubscribe(channel_up)
            await pubsub.close()
            await self.clear_auth_token(session_id)

    async def run_all_tests(self):
        """Run all integration tests."""
        print("=" * 60)
        print("WSPROXY INTEGRATION TESTS")
        print("=" * 60)

        await self.setup()

        if not await self.check_health():
            print("\n✗ Health check failed. Is wsproxy running?")
            await self.cleanup()
            return False

        test_agent = "test-agent"
        test_session = "test-session-123"
        test_token = "test-token-secret"

        tests = [
            ("WebSocket Connection", self.test_websocket_connection(test_agent, test_session, test_token)),
            ("Auth Failure", self.test_auth_failure(test_agent, "no-token-session")),
            ("Redis → WebSocket", self.test_redis_to_websocket(test_agent, test_session, test_token)),
            ("WebSocket → Redis", self.test_websocket_to_redis(test_agent, test_session, test_token)),
            ("Bidirectional", self.test_bidirectional(test_agent, test_session, test_token)),
        ]

        results = []
        for name, test in tests:
            try:
                result = await test
                results.append((name, result))
            except Exception as e:
                print(f"\n✗ Test '{name}' crashed: {e}")
                results.append((name, False))

        await self.cleanup()

        print("\n" + "=" * 60)
        print("TEST RESULTS")
        print("=" * 60)

        passed = sum(1 for _, result in results if result)
        total = len(results)

        for name, result in results:
            status = "✓ PASS" if result else "✗ FAIL"
            print(f"{status}: {name}")

        print(f"\nTotal: {passed}/{total} tests passed")
        print("=" * 60)

        return passed == total


async def main():
    tester = WsProxyTester()
    success = await tester.run_all_tests()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    asyncio.run(main())