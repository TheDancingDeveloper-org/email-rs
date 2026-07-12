from typing import Literal

AuthClientKind = Literal["bearer", "cookie"]

AUTH_CLIENT_KIND_VALUES: set[AuthClientKind] = {
    "bearer",
    "cookie",
}


def check_auth_client_kind(value: str) -> AuthClientKind:
    if value in AUTH_CLIENT_KIND_VALUES:
        return value
    raise TypeError(f"Unexpected value {value!r}. Expected one of {AUTH_CLIENT_KIND_VALUES!r}")
