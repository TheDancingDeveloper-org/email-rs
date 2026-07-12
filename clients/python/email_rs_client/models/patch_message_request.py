from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PatchMessageRequest")


@_attrs_define
class PatchMessageRequest:
    """
    Attributes:
        is_flagged (bool | None | Unset):
        is_read (bool | None | Unset):
    """

    is_flagged: bool | None | Unset = UNSET
    is_read: bool | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        is_flagged: bool | None | Unset
        if isinstance(self.is_flagged, Unset):
            is_flagged = UNSET
        else:
            is_flagged = self.is_flagged

        is_read: bool | None | Unset
        if isinstance(self.is_read, Unset):
            is_read = UNSET
        else:
            is_read = self.is_read

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if is_flagged is not UNSET:
            field_dict["isFlagged"] = is_flagged
        if is_read is not UNSET:
            field_dict["isRead"] = is_read

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_is_flagged(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        is_flagged = _parse_is_flagged(d.pop("isFlagged", UNSET))

        def _parse_is_read(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        is_read = _parse_is_read(d.pop("isRead", UNSET))

        patch_message_request = cls(
            is_flagged=is_flagged,
            is_read=is_read,
        )

        patch_message_request.additional_properties = d
        return patch_message_request

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
