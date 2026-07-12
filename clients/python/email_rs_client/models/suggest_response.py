from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.calendar_suggestion import CalendarSuggestion
    from ..models.message_suggestion import MessageSuggestion


T = TypeVar("T", bound="SuggestResponse")


@_attrs_define
class SuggestResponse:
    """
    Attributes:
        events (list[CalendarSuggestion]):
        messages (list[MessageSuggestion]):
    """

    events: list[CalendarSuggestion]
    messages: list[MessageSuggestion]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        events = []
        for events_item_data in self.events:
            events_item = events_item_data.to_dict()
            events.append(events_item)

        messages = []
        for messages_item_data in self.messages:
            messages_item = messages_item_data.to_dict()
            messages.append(messages_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "events": events,
                "messages": messages,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.calendar_suggestion import CalendarSuggestion
        from ..models.message_suggestion import MessageSuggestion

        d = dict(src_dict)
        events = []
        _events = d.pop("events")
        for events_item_data in _events:
            events_item = CalendarSuggestion.from_dict(events_item_data)

            events.append(events_item)

        messages = []
        _messages = d.pop("messages")
        for messages_item_data in _messages:
            messages_item = MessageSuggestion.from_dict(messages_item_data)

            messages.append(messages_item)

        suggest_response = cls(
            events=events,
            messages=messages,
        )

        suggest_response.additional_properties = d
        return suggest_response

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
