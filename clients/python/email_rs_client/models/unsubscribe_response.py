from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="UnsubscribeResponse")


@_attrs_define
class UnsubscribeResponse:
    """
    Attributes:
        method (str): `one_click` or `mailto`.
        http_status (int | None | Unset): HTTP status of the one-click POST, absent for `mailto`.
        outbox (Any | Unset): Outbox entry queued for `mailto`, absent for `one_click`.
    """

    method: str
    http_status: int | None | Unset = UNSET
    outbox: Any | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        method = self.method

        http_status: int | None | Unset
        if isinstance(self.http_status, Unset):
            http_status = UNSET
        else:
            http_status = self.http_status

        outbox = self.outbox

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "method": method,
            }
        )
        if http_status is not UNSET:
            field_dict["httpStatus"] = http_status
        if outbox is not UNSET:
            field_dict["outbox"] = outbox

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        method = d.pop("method")

        def _parse_http_status(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        http_status = _parse_http_status(d.pop("httpStatus", UNSET))

        outbox = d.pop("outbox", UNSET)

        unsubscribe_response = cls(
            method=method,
            http_status=http_status,
            outbox=outbox,
        )

        unsubscribe_response.additional_properties = d
        return unsubscribe_response

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
