from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="EventLink")


@_attrs_define
class EventLink:
    """
    Attributes:
        event_id (str):
        folder_id (str):
        id (str):
        linked_at (str):
        message_id (str):
        date (None | str | Unset):
        from_email (None | str | Unset):
        from_name (None | str | Unset):
        subject (None | str | Unset):
    """

    event_id: str
    folder_id: str
    id: str
    linked_at: str
    message_id: str
    date: None | str | Unset = UNSET
    from_email: None | str | Unset = UNSET
    from_name: None | str | Unset = UNSET
    subject: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        event_id = self.event_id

        folder_id = self.folder_id

        id = self.id

        linked_at = self.linked_at

        message_id = self.message_id

        date: None | str | Unset
        if isinstance(self.date, Unset):
            date = UNSET
        else:
            date = self.date

        from_email: None | str | Unset
        if isinstance(self.from_email, Unset):
            from_email = UNSET
        else:
            from_email = self.from_email

        from_name: None | str | Unset
        if isinstance(self.from_name, Unset):
            from_name = UNSET
        else:
            from_name = self.from_name

        subject: None | str | Unset
        if isinstance(self.subject, Unset):
            subject = UNSET
        else:
            subject = self.subject

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "eventId": event_id,
                "folderId": folder_id,
                "id": id,
                "linkedAt": linked_at,
                "messageId": message_id,
            }
        )
        if date is not UNSET:
            field_dict["date"] = date
        if from_email is not UNSET:
            field_dict["fromEmail"] = from_email
        if from_name is not UNSET:
            field_dict["fromName"] = from_name
        if subject is not UNSET:
            field_dict["subject"] = subject

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        event_id = d.pop("eventId")

        folder_id = d.pop("folderId")

        id = d.pop("id")

        linked_at = d.pop("linkedAt")

        message_id = d.pop("messageId")

        def _parse_date(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        date = _parse_date(d.pop("date", UNSET))

        def _parse_from_email(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        from_email = _parse_from_email(d.pop("fromEmail", UNSET))

        def _parse_from_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        from_name = _parse_from_name(d.pop("fromName", UNSET))

        def _parse_subject(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        subject = _parse_subject(d.pop("subject", UNSET))

        event_link = cls(
            event_id=event_id,
            folder_id=folder_id,
            id=id,
            linked_at=linked_at,
            message_id=message_id,
            date=date,
            from_email=from_email,
            from_name=from_name,
            subject=subject,
        )

        event_link.additional_properties = d
        return event_link

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
