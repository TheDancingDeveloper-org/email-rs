from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

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
        needs_reauth (bool | Unset): The provider refused the stored OAuth grant (`invalid_grant`): sync is
            paused until the user reconnects the account.
        reauth_reason (None | str | Unset): The provider's reason, when `needs_reauth` is set.
    """

    auth_type: str
    created_at: str
    email: str
    id: str
    name: str
    provider_type: str
    needs_reauth: bool | Unset = UNSET
    reauth_reason: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        auth_type = self.auth_type

        created_at = self.created_at

        email = self.email

        id = self.id

        name = self.name

        provider_type = self.provider_type

        needs_reauth = self.needs_reauth

        reauth_reason: None | str | Unset
        if isinstance(self.reauth_reason, Unset):
            reauth_reason = UNSET
        else:
            reauth_reason = self.reauth_reason

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
        if needs_reauth is not UNSET:
            field_dict["needsReauth"] = needs_reauth
        if reauth_reason is not UNSET:
            field_dict["reauthReason"] = reauth_reason

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

        needs_reauth = d.pop("needsReauth", UNSET)

        def _parse_reauth_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reauth_reason = _parse_reauth_reason(d.pop("reauthReason", UNSET))

        account = cls(
            auth_type=auth_type,
            created_at=created_at,
            email=email,
            id=id,
            name=name,
            provider_type=provider_type,
            needs_reauth=needs_reauth,
            reauth_reason=reauth_reason,
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
