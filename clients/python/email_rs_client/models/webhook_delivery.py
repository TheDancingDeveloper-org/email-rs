from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="WebhookDelivery")


@_attrs_define
class WebhookDelivery:
    """
    Attributes:
        attempts (int):
        event_id (str):
        next_attempt_at (str):
        status (str):
        webhook_id (str):
        delivered_at (None | str | Unset):
        last_attempt_at (None | str | Unset):
        last_error (None | str | Unset):
        response_status (int | None | Unset):
    """

    attempts: int
    event_id: str
    next_attempt_at: str
    status: str
    webhook_id: str
    delivered_at: None | str | Unset = UNSET
    last_attempt_at: None | str | Unset = UNSET
    last_error: None | str | Unset = UNSET
    response_status: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        attempts = self.attempts

        event_id = self.event_id

        next_attempt_at = self.next_attempt_at

        status = self.status

        webhook_id = self.webhook_id

        delivered_at: None | str | Unset
        if isinstance(self.delivered_at, Unset):
            delivered_at = UNSET
        else:
            delivered_at = self.delivered_at

        last_attempt_at: None | str | Unset
        if isinstance(self.last_attempt_at, Unset):
            last_attempt_at = UNSET
        else:
            last_attempt_at = self.last_attempt_at

        last_error: None | str | Unset
        if isinstance(self.last_error, Unset):
            last_error = UNSET
        else:
            last_error = self.last_error

        response_status: int | None | Unset
        if isinstance(self.response_status, Unset):
            response_status = UNSET
        else:
            response_status = self.response_status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "attempts": attempts,
                "eventId": event_id,
                "nextAttemptAt": next_attempt_at,
                "status": status,
                "webhookId": webhook_id,
            }
        )
        if delivered_at is not UNSET:
            field_dict["deliveredAt"] = delivered_at
        if last_attempt_at is not UNSET:
            field_dict["lastAttemptAt"] = last_attempt_at
        if last_error is not UNSET:
            field_dict["lastError"] = last_error
        if response_status is not UNSET:
            field_dict["responseStatus"] = response_status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        attempts = d.pop("attempts")

        event_id = d.pop("eventId")

        next_attempt_at = d.pop("nextAttemptAt")

        status = d.pop("status")

        webhook_id = d.pop("webhookId")

        def _parse_delivered_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        delivered_at = _parse_delivered_at(d.pop("deliveredAt", UNSET))

        def _parse_last_attempt_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_attempt_at = _parse_last_attempt_at(d.pop("lastAttemptAt", UNSET))

        def _parse_last_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_error = _parse_last_error(d.pop("lastError", UNSET))

        def _parse_response_status(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        response_status = _parse_response_status(d.pop("responseStatus", UNSET))

        webhook_delivery = cls(
            attempts=attempts,
            event_id=event_id,
            next_attempt_at=next_attempt_at,
            status=status,
            webhook_id=webhook_id,
            delivered_at=delivered_at,
            last_attempt_at=last_attempt_at,
            last_error=last_error,
            response_status=response_status,
        )

        webhook_delivery.additional_properties = d
        return webhook_delivery

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
