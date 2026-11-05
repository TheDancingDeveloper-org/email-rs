from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="SendResponse")


@_attrs_define
class SendResponse:
    """
    Attributes:
        status (str):
        cancel_until (None | str | Unset):
        id (None | str | Unset): Outbox id; pass it to `DELETE /outbox/{id}` to cancel before `cancel_until`.
        scheduled_at (None | str | Unset):
    """

    status: str
    cancel_until: None | str | Unset = UNSET
    id: None | str | Unset = UNSET
    scheduled_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        status = self.status

        cancel_until: None | str | Unset
        if isinstance(self.cancel_until, Unset):
            cancel_until = UNSET
        else:
            cancel_until = self.cancel_until

        id: None | str | Unset
        if isinstance(self.id, Unset):
            id = UNSET
        else:
            id = self.id

        scheduled_at: None | str | Unset
        if isinstance(self.scheduled_at, Unset):
            scheduled_at = UNSET
        else:
            scheduled_at = self.scheduled_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "status": status,
            }
        )
        if cancel_until is not UNSET:
            field_dict["cancelUntil"] = cancel_until
        if id is not UNSET:
            field_dict["id"] = id
        if scheduled_at is not UNSET:
            field_dict["scheduledAt"] = scheduled_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        status = d.pop("status")

        def _parse_cancel_until(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        cancel_until = _parse_cancel_until(d.pop("cancelUntil", UNSET))

        def _parse_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        id = _parse_id(d.pop("id", UNSET))

        def _parse_scheduled_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        scheduled_at = _parse_scheduled_at(d.pop("scheduledAt", UNSET))

        send_response = cls(
            status=status,
            cancel_until=cancel_until,
            id=id,
            scheduled_at=scheduled_at,
        )

        send_response.additional_properties = d
        return send_response

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
