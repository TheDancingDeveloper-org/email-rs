from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="UpdateIdentity")


@_attrs_define
class UpdateIdentity:
    """
    Attributes:
        email (None | str | Unset):
        name (None | str | Unset):
        signature (None | str | Unset):
        signature_html (None | str | Unset):
        signature_placement (None | str | Unset):
    """

    email: None | str | Unset = UNSET
    name: None | str | Unset = UNSET
    signature: None | str | Unset = UNSET
    signature_html: None | str | Unset = UNSET
    signature_placement: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        email: None | str | Unset
        if isinstance(self.email, Unset):
            email = UNSET
        else:
            email = self.email

        name: None | str | Unset
        if isinstance(self.name, Unset):
            name = UNSET
        else:
            name = self.name

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

        signature_placement: None | str | Unset
        if isinstance(self.signature_placement, Unset):
            signature_placement = UNSET
        else:
            signature_placement = self.signature_placement

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if email is not UNSET:
            field_dict["email"] = email
        if name is not UNSET:
            field_dict["name"] = name
        if signature is not UNSET:
            field_dict["signature"] = signature
        if signature_html is not UNSET:
            field_dict["signatureHtml"] = signature_html
        if signature_placement is not UNSET:
            field_dict["signaturePlacement"] = signature_placement

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_email(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        email = _parse_email(d.pop("email", UNSET))

        def _parse_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        name = _parse_name(d.pop("name", UNSET))

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

        def _parse_signature_placement(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        signature_placement = _parse_signature_placement(d.pop("signaturePlacement", UNSET))

        update_identity = cls(
            email=email,
            name=name,
            signature=signature,
            signature_html=signature_html,
            signature_placement=signature_placement,
        )

        update_identity.additional_properties = d
        return update_identity

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
