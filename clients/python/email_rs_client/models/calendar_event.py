from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.attendee import Attendee


T = TypeVar("T", bound="CalendarEvent")


@_attrs_define
class CalendarEvent:
    """
    Attributes:
        attendees (list[Attendee]):
        calendar_id (str):
        end_at (datetime.datetime):
        id (str):
        is_all_day (bool):
        start_at (datetime.datetime):
        title (str):
        description (None | str | Unset):
        location (None | str | Unset):
        meet_link (None | str | Unset):
        provider_event_id (None | str | Unset):
        recurrence_rule (None | str | Unset):
    """

    attendees: list[Attendee]
    calendar_id: str
    end_at: datetime.datetime
    id: str
    is_all_day: bool
    start_at: datetime.datetime
    title: str
    description: None | str | Unset = UNSET
    location: None | str | Unset = UNSET
    meet_link: None | str | Unset = UNSET
    provider_event_id: None | str | Unset = UNSET
    recurrence_rule: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        attendees = []
        for attendees_item_data in self.attendees:
            attendees_item = attendees_item_data.to_dict()
            attendees.append(attendees_item)

        calendar_id = self.calendar_id

        end_at = self.end_at.isoformat()

        id = self.id

        is_all_day = self.is_all_day

        start_at = self.start_at.isoformat()

        title = self.title

        description: None | str | Unset
        if isinstance(self.description, Unset):
            description = UNSET
        else:
            description = self.description

        location: None | str | Unset
        if isinstance(self.location, Unset):
            location = UNSET
        else:
            location = self.location

        meet_link: None | str | Unset
        if isinstance(self.meet_link, Unset):
            meet_link = UNSET
        else:
            meet_link = self.meet_link

        provider_event_id: None | str | Unset
        if isinstance(self.provider_event_id, Unset):
            provider_event_id = UNSET
        else:
            provider_event_id = self.provider_event_id

        recurrence_rule: None | str | Unset
        if isinstance(self.recurrence_rule, Unset):
            recurrence_rule = UNSET
        else:
            recurrence_rule = self.recurrence_rule

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "attendees": attendees,
                "calendarId": calendar_id,
                "endAt": end_at,
                "id": id,
                "isAllDay": is_all_day,
                "startAt": start_at,
                "title": title,
            }
        )
        if description is not UNSET:
            field_dict["description"] = description
        if location is not UNSET:
            field_dict["location"] = location
        if meet_link is not UNSET:
            field_dict["meetLink"] = meet_link
        if provider_event_id is not UNSET:
            field_dict["providerEventId"] = provider_event_id
        if recurrence_rule is not UNSET:
            field_dict["recurrenceRule"] = recurrence_rule

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.attendee import Attendee

        d = dict(src_dict)
        attendees = []
        _attendees = d.pop("attendees")
        for attendees_item_data in _attendees:
            attendees_item = Attendee.from_dict(attendees_item_data)

            attendees.append(attendees_item)

        calendar_id = d.pop("calendarId")

        end_at = datetime.datetime.fromisoformat(d.pop("endAt"))

        id = d.pop("id")

        is_all_day = d.pop("isAllDay")

        start_at = datetime.datetime.fromisoformat(d.pop("startAt"))

        title = d.pop("title")

        def _parse_description(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        description = _parse_description(d.pop("description", UNSET))

        def _parse_location(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        location = _parse_location(d.pop("location", UNSET))

        def _parse_meet_link(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        meet_link = _parse_meet_link(d.pop("meetLink", UNSET))

        def _parse_provider_event_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        provider_event_id = _parse_provider_event_id(d.pop("providerEventId", UNSET))

        def _parse_recurrence_rule(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        recurrence_rule = _parse_recurrence_rule(d.pop("recurrenceRule", UNSET))

        calendar_event = cls(
            attendees=attendees,
            calendar_id=calendar_id,
            end_at=end_at,
            id=id,
            is_all_day=is_all_day,
            start_at=start_at,
            title=title,
            description=description,
            location=location,
            meet_link=meet_link,
            provider_event_id=provider_event_id,
            recurrence_rule=recurrence_rule,
        )

        calendar_event.additional_properties = d
        return calendar_event

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
