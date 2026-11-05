from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="Identity")


@_attrs_define
class Identity:
    """An address a message can be sent From. Each account has a default identity
    plus any aliases added to it.

        Attributes:
            account_id (str):
            email (str):
            id (str):
            is_default (bool):
            name (str):
    """

    account_id: str
    email: str
    id: str
    is_default: bool
    name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        email = self.email

        id = self.id

        is_default = self.is_default

        name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "email": email,
                "id": id,
                "isDefault": is_default,
                "name": name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        email = d.pop("email")

        id = d.pop("id")

        is_default = d.pop("isDefault")

        name = d.pop("name")

        identity = cls(
            account_id=account_id,
            email=email,
            id=id,
            is_default=is_default,
            name=name,
        )

        identity.additional_properties = d
        return identity

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
