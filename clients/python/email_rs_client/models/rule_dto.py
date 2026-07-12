from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.rule_action_dto import RuleActionDto
    from ..models.rule_condition_dto import RuleConditionDto


T = TypeVar("T", bound="RuleDto")


@_attrs_define
class RuleDto:
    """
    Attributes:
        account_id (str):
        actions (list[RuleActionDto]):
        conditions (list[RuleConditionDto]):
        id (str):
        is_active (bool):
        match_mode (str):
        name (str):
        priority (int):
    """

    account_id: str
    actions: list[RuleActionDto]
    conditions: list[RuleConditionDto]
    id: str
    is_active: bool
    match_mode: str
    name: str
    priority: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        actions = []
        for actions_item_data in self.actions:
            actions_item = actions_item_data.to_dict()
            actions.append(actions_item)

        conditions = []
        for conditions_item_data in self.conditions:
            conditions_item = conditions_item_data.to_dict()
            conditions.append(conditions_item)

        id = self.id

        is_active = self.is_active

        match_mode = self.match_mode

        name = self.name

        priority = self.priority

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "actions": actions,
                "conditions": conditions,
                "id": id,
                "isActive": is_active,
                "matchMode": match_mode,
                "name": name,
                "priority": priority,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.rule_action_dto import RuleActionDto
        from ..models.rule_condition_dto import RuleConditionDto

        d = dict(src_dict)
        account_id = d.pop("accountId")

        actions = []
        _actions = d.pop("actions")
        for actions_item_data in _actions:
            actions_item = RuleActionDto.from_dict(actions_item_data)

            actions.append(actions_item)

        conditions = []
        _conditions = d.pop("conditions")
        for conditions_item_data in _conditions:
            conditions_item = RuleConditionDto.from_dict(conditions_item_data)

            conditions.append(conditions_item)

        id = d.pop("id")

        is_active = d.pop("isActive")

        match_mode = d.pop("matchMode")

        name = d.pop("name")

        priority = d.pop("priority")

        rule_dto = cls(
            account_id=account_id,
            actions=actions,
            conditions=conditions,
            id=id,
            is_active=is_active,
            match_mode=match_mode,
            name=name,
            priority=priority,
        )

        rule_dto.additional_properties = d
        return rule_dto

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
