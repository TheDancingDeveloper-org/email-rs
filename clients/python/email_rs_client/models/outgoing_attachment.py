from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OutgoingAttachment")


@_attrs_define
class OutgoingAttachment:
    """
    Attributes:
        content_base_64 (str): Standard base64-encoded file content.
        content_type (str):
        filename (str):
    """

    content_base_64: str
    content_type: str
    filename: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content_base_64 = self.content_base_64

        content_type = self.content_type

        filename = self.filename

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "contentBase64": content_base_64,
                "contentType": content_type,
                "filename": filename,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        content_base_64 = d.pop("contentBase64")

        content_type = d.pop("contentType")

        filename = d.pop("filename")

        outgoing_attachment = cls(
            content_base_64=content_base_64,
            content_type=content_type,
            filename=filename,
        )

        outgoing_attachment.additional_properties = d
        return outgoing_attachment

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
