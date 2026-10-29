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
        clean_body (None | str | Unset): The current message as Markdown for models and search: the real text
            part (or the HTML converted), without quoted history, signature,
            images or tracking links. Computed once when the body is cached.
        html_body (None | str | Unset):
        quoted_history (None | str | Unset): The quoted history split off `cleanBody`, if the message had any.
        signature_stripped (bool | Unset): Whether a signature was removed from `cleanBody`.
        text_body (None | str | Unset):
    """

    clean_body: None | str | Unset = UNSET
    html_body: None | str | Unset = UNSET
    quoted_history: None | str | Unset = UNSET
    signature_stripped: bool | Unset = UNSET
    text_body: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        clean_body: None | str | Unset
        if isinstance(self.clean_body, Unset):
            clean_body = UNSET
        else:
            clean_body = self.clean_body

        html_body: None | str | Unset
        if isinstance(self.html_body, Unset):
            html_body = UNSET
        else:
            html_body = self.html_body

        quoted_history: None | str | Unset
        if isinstance(self.quoted_history, Unset):
            quoted_history = UNSET
        else:
            quoted_history = self.quoted_history

        signature_stripped = self.signature_stripped

        text_body: None | str | Unset
        if isinstance(self.text_body, Unset):
            text_body = UNSET
        else:
            text_body = self.text_body

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if clean_body is not UNSET:
            field_dict["cleanBody"] = clean_body
        if html_body is not UNSET:
            field_dict["htmlBody"] = html_body
        if quoted_history is not UNSET:
            field_dict["quotedHistory"] = quoted_history
        if signature_stripped is not UNSET:
            field_dict["signatureStripped"] = signature_stripped
        if text_body is not UNSET:
            field_dict["textBody"] = text_body

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_clean_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        clean_body = _parse_clean_body(d.pop("cleanBody", UNSET))

        def _parse_html_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        html_body = _parse_html_body(d.pop("htmlBody", UNSET))

        def _parse_quoted_history(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        quoted_history = _parse_quoted_history(d.pop("quotedHistory", UNSET))

        signature_stripped = d.pop("signatureStripped", UNSET)

        def _parse_text_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        text_body = _parse_text_body(d.pop("textBody", UNSET))

        message_body = cls(
            clean_body=clean_body,
            html_body=html_body,
            quoted_history=quoted_history,
            signature_stripped=signature_stripped,
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
