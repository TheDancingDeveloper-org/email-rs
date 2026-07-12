from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreatedIntegrationToken")


@_attrs_define
class CreatedIntegrationToken:
    """
    Attributes:
        created_at (str):
        id (str):
        name (str):
        scopes (list[str]):
        token_prefix (str):
        token (str): Returned exactly once. Only its SHA-256 hash is persisted.
        expires_at (int | None | Unset):
        last_used_at (None | str | Unset):
        revoked_at (None | str | Unset):
    """

    created_at: str
    id: str
    name: str
    scopes: list[str]
    token_prefix: str
    token: str
    expires_at: int | None | Unset = UNSET
    last_used_at: None | str | Unset = UNSET
    revoked_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        id = self.id

        name = self.name

        scopes = self.scopes

        token_prefix = self.token_prefix

        token = self.token

        expires_at: int | None | Unset
        if isinstance(self.expires_at, Unset):
            expires_at = UNSET
        else:
            expires_at = self.expires_at

        last_used_at: None | str | Unset
        if isinstance(self.last_used_at, Unset):
            last_used_at = UNSET
        else:
            last_used_at = self.last_used_at

        revoked_at: None | str | Unset
        if isinstance(self.revoked_at, Unset):
            revoked_at = UNSET
        else:
            revoked_at = self.revoked_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "createdAt": created_at,
                "id": id,
                "name": name,
                "scopes": scopes,
                "tokenPrefix": token_prefix,
                "token": token,
            }
        )
        if expires_at is not UNSET:
            field_dict["expiresAt"] = expires_at
        if last_used_at is not UNSET:
            field_dict["lastUsedAt"] = last_used_at
        if revoked_at is not UNSET:
            field_dict["revokedAt"] = revoked_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("createdAt")

        id = d.pop("id")

        name = d.pop("name")

        scopes = cast(list[str], d.pop("scopes"))

        token_prefix = d.pop("tokenPrefix")

        token = d.pop("token")

        def _parse_expires_at(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        expires_at = _parse_expires_at(d.pop("expiresAt", UNSET))

        def _parse_last_used_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_used_at = _parse_last_used_at(d.pop("lastUsedAt", UNSET))

        def _parse_revoked_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        revoked_at = _parse_revoked_at(d.pop("revokedAt", UNSET))

        created_integration_token = cls(
            created_at=created_at,
            id=id,
            name=name,
            scopes=scopes,
            token_prefix=token_prefix,
            token=token,
            expires_at=expires_at,
            last_used_at=last_used_at,
            revoked_at=revoked_at,
        )

        created_integration_token.additional_properties = d
        return created_integration_token

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
