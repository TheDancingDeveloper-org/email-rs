from typing import Literal

AuthenticationResultStatus = Literal["fail", "none", "pass"]

AUTHENTICATION_RESULT_STATUS_VALUES: set[AuthenticationResultStatus] = {
    "fail",
    "none",
    "pass",
}


def check_authentication_result_status(value: str) -> AuthenticationResultStatus:
    if value in AUTHENTICATION_RESULT_STATUS_VALUES:
        return value
    raise TypeError(f"Unexpected value {value!r}. Expected one of {AUTHENTICATION_RESULT_STATUS_VALUES!r}")
