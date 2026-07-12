from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="RuleActionDto")


@_attrs_define
class RuleActionDto:
    """
    Attributes:
        action_type (str):
        id (str):
        action_value (None | str | Unset):
    """

    action_type: str
    id: str
    action_value: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action_type = self.action_type

        id = self.id

        action_value: None | str | Unset
        if isinstance(self.action_value, Unset):
            action_value = UNSET
        else:
            action_value = self.action_value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "actionType": action_type,
                "id": id,
            }
        )
        if action_value is not UNSET:
            field_dict["actionValue"] = action_value

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        action_type = d.pop("actionType")

        id = d.pop("id")

        def _parse_action_value(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        action_value = _parse_action_value(d.pop("actionValue", UNSET))

        rule_action_dto = cls(
            action_type=action_type,
            id=id,
            action_value=action_value,
        )

        rule_action_dto.additional_properties = d
        return rule_action_dto

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
