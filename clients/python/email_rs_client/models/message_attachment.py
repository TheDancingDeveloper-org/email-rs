from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="MessageAttachment")


@_attrs_define
class MessageAttachment:
    """
    Attributes:
        content_type (str):
        filename (str):
        id (str):
        message_id (str):
        size_bytes (int):
    """

    content_type: str
    filename: str
    id: str
    message_id: str
    size_bytes: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content_type = self.content_type

        filename = self.filename

        id = self.id

        message_id = self.message_id

        size_bytes = self.size_bytes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "contentType": content_type,
                "filename": filename,
                "id": id,
                "messageId": message_id,
                "sizeBytes": size_bytes,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        content_type = d.pop("contentType")

        filename = d.pop("filename")

        id = d.pop("id")

        message_id = d.pop("messageId")

        size_bytes = d.pop("sizeBytes")

        message_attachment = cls(
            content_type=content_type,
            filename=filename,
            id=id,
            message_id=message_id,
            size_bytes=size_bytes,
        )

        message_attachment.additional_properties = d
        return message_attachment

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
