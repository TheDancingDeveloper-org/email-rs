from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.action_input import ActionInput
    from ..models.condition_input import ConditionInput


T = TypeVar("T", bound="CreateRuleBody")


@_attrs_define
class CreateRuleBody:
    """
    Attributes:
        account_id (str):
        name (str):
        actions (list[ActionInput] | None | Unset):
        conditions (list[ConditionInput] | None | Unset):
        match_mode (None | str | Unset):
    """

    account_id: str
    name: str
    actions: list[ActionInput] | None | Unset = UNSET
    conditions: list[ConditionInput] | None | Unset = UNSET
    match_mode: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        name = self.name

        actions: list[dict[str, Any]] | None | Unset
        if isinstance(self.actions, Unset):
            actions = UNSET
        elif isinstance(self.actions, list):
            actions = []
            for actions_type_0_item_data in self.actions:
                actions_type_0_item = actions_type_0_item_data.to_dict()
                actions.append(actions_type_0_item)

        else:
            actions = self.actions

        conditions: list[dict[str, Any]] | None | Unset
        if isinstance(self.conditions, Unset):
            conditions = UNSET
        elif isinstance(self.conditions, list):
            conditions = []
            for conditions_type_0_item_data in self.conditions:
                conditions_type_0_item = conditions_type_0_item_data.to_dict()
                conditions.append(conditions_type_0_item)

        else:
            conditions = self.conditions

        match_mode: None | str | Unset
        if isinstance(self.match_mode, Unset):
            match_mode = UNSET
        else:
            match_mode = self.match_mode

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "name": name,
            }
        )
        if actions is not UNSET:
            field_dict["actions"] = actions
        if conditions is not UNSET:
            field_dict["conditions"] = conditions
        if match_mode is not UNSET:
            field_dict["matchMode"] = match_mode

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.action_input import ActionInput
        from ..models.condition_input import ConditionInput

        d = dict(src_dict)
        account_id = d.pop("accountId")

        name = d.pop("name")

        def _parse_actions(data: object) -> list[ActionInput] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                actions_type_0 = []
                _actions_type_0 = data
                for actions_type_0_item_data in _actions_type_0:
                    actions_type_0_item = ActionInput.from_dict(actions_type_0_item_data)

                    actions_type_0.append(actions_type_0_item)

                return actions_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[ActionInput] | None | Unset, data)

        actions = _parse_actions(d.pop("actions", UNSET))

        def _parse_conditions(data: object) -> list[ConditionInput] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                conditions_type_0 = []
                _conditions_type_0 = data
                for conditions_type_0_item_data in _conditions_type_0:
                    conditions_type_0_item = ConditionInput.from_dict(conditions_type_0_item_data)

                    conditions_type_0.append(conditions_type_0_item)

                return conditions_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[ConditionInput] | None | Unset, data)

        conditions = _parse_conditions(d.pop("conditions", UNSET))

        def _parse_match_mode(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        match_mode = _parse_match_mode(d.pop("matchMode", UNSET))

        create_rule_body = cls(
            account_id=account_id,
            name=name,
            actions=actions,
            conditions=conditions,
            match_mode=match_mode,
        )

        create_rule_body.additional_properties = d
        return create_rule_body

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
