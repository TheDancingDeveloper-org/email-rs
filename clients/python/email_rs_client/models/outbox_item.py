from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="OutboxItem")


@_attrs_define
class OutboxItem:
    """A queued send, as listed by `GET /outbox`.

    Attributes:
        attempts (int):
        cancel_until (str):
        id (str):
        scheduled_at (str):
        status (str): `pending`, `sending`, `sent` or `cancelled`.
        last_error (None | str | Unset):
    """

    attempts: int
    cancel_until: str
    id: str
    scheduled_at: str
    status: str
    last_error: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        attempts = self.attempts

        cancel_until = self.cancel_until

        id = self.id

        scheduled_at = self.scheduled_at

        status = self.status

        last_error: None | str | Unset
        if isinstance(self.last_error, Unset):
            last_error = UNSET
        else:
            last_error = self.last_error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "attempts": attempts,
                "cancelUntil": cancel_until,
                "id": id,
                "scheduledAt": scheduled_at,
                "status": status,
            }
        )
        if last_error is not UNSET:
            field_dict["lastError"] = last_error

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        attempts = d.pop("attempts")

        cancel_until = d.pop("cancelUntil")

        id = d.pop("id")

        scheduled_at = d.pop("scheduledAt")

        status = d.pop("status")

        def _parse_last_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_error = _parse_last_error(d.pop("lastError", UNSET))

        outbox_item = cls(
            attempts=attempts,
            cancel_until=cancel_until,
            id=id,
            scheduled_at=scheduled_at,
            status=status,
            last_error=last_error,
        )

        outbox_item.additional_properties = d
        return outbox_item

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
