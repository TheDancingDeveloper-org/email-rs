from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ListUnsubscribe")


@_attrs_define
class ListUnsubscribe:
    """Unsubscribe targets advertised by a message's `List-Unsubscribe` header
    (RFC 2369), with the RFC 8058 one-click flag from `List-Unsubscribe-Post`.

        Attributes:
            https (list[str]): `https:` targets. Plain `http:` targets are never offered.
            mailto (list[str]):
            one_click (bool): True when `List-Unsubscribe-Post: List-Unsubscribe=One-Click` is present
                and at least one HTTPS target exists.
    """

    https: list[str]
    mailto: list[str]
    one_click: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        https = self.https

        mailto = self.mailto

        one_click = self.one_click

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "https": https,
                "mailto": mailto,
                "oneClick": one_click,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        https = cast(list[str], d.pop("https"))

        mailto = cast(list[str], d.pop("mailto"))

        one_click = d.pop("oneClick")

        list_unsubscribe = cls(
            https=https,
            mailto=mailto,
            one_click=one_click,
        )

        list_unsubscribe.additional_properties = d
        return list_unsubscribe

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
