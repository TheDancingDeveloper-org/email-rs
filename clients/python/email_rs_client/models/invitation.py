from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.invitation_attendee import InvitationAttendee


T = TypeVar("T", bound="Invitation")


@_attrs_define
class Invitation:
    """
    Attributes:
        attendees (list[InvitationAttendee]):
        end_at (str):
        message_id (str):
        method (str):
        start_at (str):
        uid (str):
        description (None | str | Unset):
        location (None | str | Unset):
        organizer (None | str | Unset):
        sequence (int | None | Unset):
        summary (None | str | Unset):
    """

    attendees: list[InvitationAttendee]
    end_at: str
    message_id: str
    method: str
    start_at: str
    uid: str
    description: None | str | Unset = UNSET
    location: None | str | Unset = UNSET
    organizer: None | str | Unset = UNSET
    sequence: int | None | Unset = UNSET
    summary: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        attendees = []
        for attendees_item_data in self.attendees:
            attendees_item = attendees_item_data.to_dict()
            attendees.append(attendees_item)

        end_at = self.end_at

        message_id = self.message_id

        method = self.method

        start_at = self.start_at

        uid = self.uid

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

        organizer: None | str | Unset
        if isinstance(self.organizer, Unset):
            organizer = UNSET
        else:
            organizer = self.organizer

        sequence: int | None | Unset
        if isinstance(self.sequence, Unset):
            sequence = UNSET
        else:
            sequence = self.sequence

        summary: None | str | Unset
        if isinstance(self.summary, Unset):
            summary = UNSET
        else:
            summary = self.summary

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "attendees": attendees,
                "endAt": end_at,
                "messageId": message_id,
                "method": method,
                "startAt": start_at,
                "uid": uid,
            }
        )
        if description is not UNSET:
            field_dict["description"] = description
        if location is not UNSET:
            field_dict["location"] = location
        if organizer is not UNSET:
            field_dict["organizer"] = organizer
        if sequence is not UNSET:
            field_dict["sequence"] = sequence
        if summary is not UNSET:
            field_dict["summary"] = summary

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.invitation_attendee import InvitationAttendee  # noqa: PLC0415

        d = dict(src_dict)
        attendees = []
        _attendees = d.pop("attendees")
        for attendees_item_data in _attendees:
            attendees_item = InvitationAttendee.from_dict(attendees_item_data)

            attendees.append(attendees_item)

        end_at = d.pop("endAt")

        message_id = d.pop("messageId")

        method = d.pop("method")

        start_at = d.pop("startAt")

        uid = d.pop("uid")

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

        def _parse_organizer(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        organizer = _parse_organizer(d.pop("organizer", UNSET))

        def _parse_sequence(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        sequence = _parse_sequence(d.pop("sequence", UNSET))

        def _parse_summary(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        summary = _parse_summary(d.pop("summary", UNSET))

        invitation = cls(
            attendees=attendees,
            end_at=end_at,
            message_id=message_id,
            method=method,
            start_at=start_at,
            uid=uid,
            description=description,
            location=location,
            organizer=organizer,
            sequence=sequence,
            summary=summary,
        )

        invitation.additional_properties = d
        return invitation

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
