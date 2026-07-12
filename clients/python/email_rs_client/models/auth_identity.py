from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.auth_client_kind import AuthClientKind, check_auth_client_kind

T = TypeVar("T", bound="AuthIdentity")


@_attrs_define
class AuthIdentity:
    """
    Attributes:
        client (AuthClientKind):
        session_id (str):
        user_id (str):
    """

    client: AuthClientKind
    session_id: str
    user_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        client: str = self.client

        session_id = self.session_id

        user_id = self.user_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "client": client,
                "sessionId": session_id,
                "userId": user_id,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        client = check_auth_client_kind(d.pop("client"))

        session_id = d.pop("sessionId")

        user_id = d.pop("userId")

        auth_identity = cls(
            client=client,
            session_id=session_id,
            user_id=user_id,
        )

        auth_identity.additional_properties = d
        return auth_identity

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
