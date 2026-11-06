from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AccountHealth")


@_attrs_define
class AccountHealth:
    """
    Attributes:
        account_id (str):
        needs_reauth (bool):
        offline_queue_depth (int):
        queue_depth (int): Outgoing messages pending or currently sending, including scheduled sends.
        last_error (None | str | Unset): Safe error category, cleared on recovery. Never raw provider diagnostics.
        last_error_at (None | str | Unset):
        last_success_at (None | str | Unset): NULL until a complete mail sync succeeds; calendar sync is separate.
    """

    account_id: str
    needs_reauth: bool
    offline_queue_depth: int
    queue_depth: int
    last_error: None | str | Unset = UNSET
    last_error_at: None | str | Unset = UNSET
    last_success_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        needs_reauth = self.needs_reauth

        offline_queue_depth = self.offline_queue_depth

        queue_depth = self.queue_depth

        last_error: None | str | Unset
        if isinstance(self.last_error, Unset):
            last_error = UNSET
        else:
            last_error = self.last_error

        last_error_at: None | str | Unset
        if isinstance(self.last_error_at, Unset):
            last_error_at = UNSET
        else:
            last_error_at = self.last_error_at

        last_success_at: None | str | Unset
        if isinstance(self.last_success_at, Unset):
            last_success_at = UNSET
        else:
            last_success_at = self.last_success_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "needsReauth": needs_reauth,
                "offlineQueueDepth": offline_queue_depth,
                "queueDepth": queue_depth,
            }
        )
        if last_error is not UNSET:
            field_dict["lastError"] = last_error
        if last_error_at is not UNSET:
            field_dict["lastErrorAt"] = last_error_at
        if last_success_at is not UNSET:
            field_dict["lastSuccessAt"] = last_success_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        needs_reauth = d.pop("needsReauth")

        offline_queue_depth = d.pop("offlineQueueDepth")

        queue_depth = d.pop("queueDepth")

        def _parse_last_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_error = _parse_last_error(d.pop("lastError", UNSET))

        def _parse_last_error_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_error_at = _parse_last_error_at(d.pop("lastErrorAt", UNSET))

        def _parse_last_success_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_success_at = _parse_last_success_at(d.pop("lastSuccessAt", UNSET))

        account_health = cls(
            account_id=account_id,
            needs_reauth=needs_reauth,
            offline_queue_depth=offline_queue_depth,
            queue_depth=queue_depth,
            last_error=last_error,
            last_error_at=last_error_at,
            last_success_at=last_success_at,
        )

        account_health.additional_properties = d
        return account_health

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
