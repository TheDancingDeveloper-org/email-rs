from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

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
            signature_placement (str):
            signature (None | str | Unset):
            signature_html (None | str | Unset):
    """

    account_id: str
    email: str
    id: str
    is_default: bool
    name: str
    signature_placement: str
    signature: None | str | Unset = UNSET
    signature_html: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        email = self.email

        id = self.id

        is_default = self.is_default

        name = self.name

        signature_placement = self.signature_placement

        signature: None | str | Unset
        if isinstance(self.signature, Unset):
            signature = UNSET
        else:
            signature = self.signature

        signature_html: None | str | Unset
        if isinstance(self.signature_html, Unset):
            signature_html = UNSET
        else:
            signature_html = self.signature_html

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "email": email,
                "id": id,
                "isDefault": is_default,
                "name": name,
                "signaturePlacement": signature_placement,
            }
        )
        if signature is not UNSET:
            field_dict["signature"] = signature
        if signature_html is not UNSET:
            field_dict["signatureHtml"] = signature_html

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        email = d.pop("email")

        id = d.pop("id")

        is_default = d.pop("isDefault")

        name = d.pop("name")

        signature_placement = d.pop("signaturePlacement")

        def _parse_signature(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        signature = _parse_signature(d.pop("signature", UNSET))

        def _parse_signature_html(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        signature_html = _parse_signature_html(d.pop("signatureHtml", UNSET))

        identity = cls(
            account_id=account_id,
            email=email,
            id=id,
            is_default=is_default,
            name=name,
            signature_placement=signature_placement,
            signature=signature,
            signature_html=signature_html,
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
