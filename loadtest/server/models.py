from pydantic import BaseModel


class SessionCreateResponse(BaseModel):
    session_id: str
    auth_token: str


class SessionCloseResponse(BaseModel):
    status: str


class RunRequest(BaseModel):
    query: str


class RunResponse(BaseModel):
    status: str


class HealthResponse(BaseModel):
    status: str
