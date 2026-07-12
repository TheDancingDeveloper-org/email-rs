from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="DraftResponse")


@_attrs_define
class DraftResponse:
    """
    Attributes:
        id (str):
        logical_id (str):
        message_id (str):
        occurrence_id (str):
        status (str):
    """

    id: str
    logical_id: str
    message_id: str
    occurrence_id: str
    status: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = self.id

        logical_id = self.logical_id

        message_id = self.message_id

        occurrence_id = self.occurrence_id

        status = self.status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "logicalId": logical_id,
                "messageId": message_id,
                "occurrenceId": occurrence_id,
                "status": status,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = d.pop("id")

        logical_id = d.pop("logicalId")

        message_id = d.pop("messageId")

        occurrence_id = d.pop("occurrenceId")

        status = d.pop("status")

        draft_response = cls(
            id=id,
            logical_id=logical_id,
            message_id=message_id,
            occurrence_id=occurrence_id,
            status=status,
        )

        draft_response.additional_properties = d
        return draft_response

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
