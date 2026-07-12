from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="MessageBody")


@_attrs_define
class MessageBody:
    """
    Attributes:
        html_body (None | str | Unset):
        text_body (None | str | Unset):
    """

    html_body: None | str | Unset = UNSET
    text_body: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        html_body: None | str | Unset
        if isinstance(self.html_body, Unset):
            html_body = UNSET
        else:
            html_body = self.html_body

        text_body: None | str | Unset
        if isinstance(self.text_body, Unset):
            text_body = UNSET
        else:
            text_body = self.text_body

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if html_body is not UNSET:
            field_dict["htmlBody"] = html_body
        if text_body is not UNSET:
            field_dict["textBody"] = text_body

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_html_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        html_body = _parse_html_body(d.pop("htmlBody", UNSET))

        def _parse_text_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        text_body = _parse_text_body(d.pop("textBody", UNSET))

        message_body = cls(
            html_body=html_body,
            text_body=text_body,
        )

        message_body.additional_properties = d
        return message_body

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
