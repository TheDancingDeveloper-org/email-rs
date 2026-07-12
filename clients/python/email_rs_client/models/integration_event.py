from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="IntegrationEvent")


@_attrs_define
class IntegrationEvent:
    """
    Attributes:
        account_id (str):
        created_at (str):
        data (Any):
        event_type (str):
        id (str):
    """

    account_id: str
    created_at: str
    data: Any
    event_type: str
    id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        created_at = self.created_at

        data = self.data

        event_type = self.event_type

        id = self.id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "createdAt": created_at,
                "data": data,
                "eventType": event_type,
                "id": id,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        created_at = d.pop("createdAt")

        data = d.pop("data")

        event_type = d.pop("eventType")

        id = d.pop("id")

        integration_event = cls(
            account_id=account_id,
            created_at=created_at,
            data=data,
            event_type=event_type,
            id=id,
        )

        integration_event.additional_properties = d
        return integration_event

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
