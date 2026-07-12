from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AccountSettings")


@_attrs_define
class AccountSettings:
    """
    Attributes:
        email (str):
        id (str):
        name (str):
        provider_type (str):
        signature (None | str | Unset):
        sync_days_limit (int | None | Unset):
    """

    email: str
    id: str
    name: str
    provider_type: str
    signature: None | str | Unset = UNSET
    sync_days_limit: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        email = self.email

        id = self.id

        name = self.name

        provider_type = self.provider_type

        signature: None | str | Unset
        if isinstance(self.signature, Unset):
            signature = UNSET
        else:
            signature = self.signature

        sync_days_limit: int | None | Unset
        if isinstance(self.sync_days_limit, Unset):
            sync_days_limit = UNSET
        else:
            sync_days_limit = self.sync_days_limit

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "email": email,
                "id": id,
                "name": name,
                "providerType": provider_type,
            }
        )
        if signature is not UNSET:
            field_dict["signature"] = signature
        if sync_days_limit is not UNSET:
            field_dict["syncDaysLimit"] = sync_days_limit

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        email = d.pop("email")

        id = d.pop("id")

        name = d.pop("name")

        provider_type = d.pop("providerType")

        def _parse_signature(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        signature = _parse_signature(d.pop("signature", UNSET))

        def _parse_sync_days_limit(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        sync_days_limit = _parse_sync_days_limit(d.pop("syncDaysLimit", UNSET))

        account_settings = cls(
            email=email,
            id=id,
            name=name,
            provider_type=provider_type,
            signature=signature,
            sync_days_limit=sync_days_limit,
        )

        account_settings.additional_properties = d
        return account_settings

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
