from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="Folder")


@_attrs_define
class Folder:
    """
    Attributes:
        account_id (str):
        full_path (str):
        id (str):
        is_excluded (bool):
        name (str):
        total_count (int):
        unread_count (int):
        special_use (None | str | Unset):
        synced_at (None | str | Unset):
    """

    account_id: str
    full_path: str
    id: str
    is_excluded: bool
    name: str
    total_count: int
    unread_count: int
    special_use: None | str | Unset = UNSET
    synced_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        full_path = self.full_path

        id = self.id

        is_excluded = self.is_excluded

        name = self.name

        total_count = self.total_count

        unread_count = self.unread_count

        special_use: None | str | Unset
        if isinstance(self.special_use, Unset):
            special_use = UNSET
        else:
            special_use = self.special_use

        synced_at: None | str | Unset
        if isinstance(self.synced_at, Unset):
            synced_at = UNSET
        else:
            synced_at = self.synced_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "fullPath": full_path,
                "id": id,
                "isExcluded": is_excluded,
                "name": name,
                "totalCount": total_count,
                "unreadCount": unread_count,
            }
        )
        if special_use is not UNSET:
            field_dict["specialUse"] = special_use
        if synced_at is not UNSET:
            field_dict["syncedAt"] = synced_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        full_path = d.pop("fullPath")

        id = d.pop("id")

        is_excluded = d.pop("isExcluded")

        name = d.pop("name")

        total_count = d.pop("totalCount")

        unread_count = d.pop("unreadCount")

        def _parse_special_use(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        special_use = _parse_special_use(d.pop("specialUse", UNSET))

        def _parse_synced_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        synced_at = _parse_synced_at(d.pop("syncedAt", UNSET))

        folder = cls(
            account_id=account_id,
            full_path=full_path,
            id=id,
            is_excluded=is_excluded,
            name=name,
            total_count=total_count,
            unread_count=unread_count,
            special_use=special_use,
            synced_at=synced_at,
        )

        folder.additional_properties = d
        return folder

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
