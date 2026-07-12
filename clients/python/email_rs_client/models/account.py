from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="Account")


@_attrs_define
class Account:
    """
    Attributes:
        auth_type (str):
        created_at (str):
        email (str):
        id (str):
        name (str):
        provider_type (str):
    """

    auth_type: str
    created_at: str
    email: str
    id: str
    name: str
    provider_type: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        auth_type = self.auth_type

        created_at = self.created_at

        email = self.email

        id = self.id

        name = self.name

        provider_type = self.provider_type

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "authType": auth_type,
                "createdAt": created_at,
                "email": email,
                "id": id,
                "name": name,
                "providerType": provider_type,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        auth_type = d.pop("authType")

        created_at = d.pop("createdAt")

        email = d.pop("email")

        id = d.pop("id")

        name = d.pop("name")

        provider_type = d.pop("providerType")

        account = cls(
            auth_type=auth_type,
            created_at=created_at,
            email=email,
            id=id,
            name=name,
            provider_type=provider_type,
        )

        account.additional_properties = d
        return account

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
