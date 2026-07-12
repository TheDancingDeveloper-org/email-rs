from typing import Literal

BulkAction = Literal["archive", "delete", "mark_read", "mark_unread"]

BULK_ACTION_VALUES: set[BulkAction] = {
    "archive",
    "delete",
    "mark_read",
    "mark_unread",
}


def check_bulk_action(value: str) -> BulkAction:
    if value in BULK_ACTION_VALUES:
        return value
    raise TypeError(f"Unexpected value {value!r}. Expected one of {BULK_ACTION_VALUES!r}")
