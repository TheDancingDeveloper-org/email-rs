from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="Message")


@_attrs_define
class Message:
    """
    Attributes:
        account_id (str):
        folder_id (str):
        has_attachments (bool):
        id (str): Backward-compatible mailbox-occurrence identifier.
        is_draft (bool):
        is_flagged (bool):
        is_read (bool):
        logical_id (str): Durable cross-folder identifier used by integrations.
        occurrence_id (str): Identifies this concrete folder/UID occurrence.
        uid (int):
        date (None | str | Unset):
        from_email (None | str | Unset):
        from_name (None | str | Unset):
        message_id (None | str | Unset):
        preview (None | str | Unset):
        snoozed_until (None | str | Unset):
        subject (None | str | Unset):
        thread_id (None | str | Unset):
        to_json (None | str | Unset):
    """

    account_id: str
    folder_id: str
    has_attachments: bool
    id: str
    is_draft: bool
    is_flagged: bool
    is_read: bool
    logical_id: str
    occurrence_id: str
    uid: int
    date: None | str | Unset = UNSET
    from_email: None | str | Unset = UNSET
    from_name: None | str | Unset = UNSET
    message_id: None | str | Unset = UNSET
    preview: None | str | Unset = UNSET
    snoozed_until: None | str | Unset = UNSET
    subject: None | str | Unset = UNSET
    thread_id: None | str | Unset = UNSET
    to_json: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        folder_id = self.folder_id

        has_attachments = self.has_attachments

        id = self.id

        is_draft = self.is_draft

        is_flagged = self.is_flagged

        is_read = self.is_read

        logical_id = self.logical_id

        occurrence_id = self.occurrence_id

        uid = self.uid

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

        message_id: None | str | Unset
        if isinstance(self.message_id, Unset):
            message_id = UNSET
        else:
            message_id = self.message_id

        preview: None | str | Unset
        if isinstance(self.preview, Unset):
            preview = UNSET
        else:
            preview = self.preview

        snoozed_until: None | str | Unset
        if isinstance(self.snoozed_until, Unset):
            snoozed_until = UNSET
        else:
            snoozed_until = self.snoozed_until

        subject: None | str | Unset
        if isinstance(self.subject, Unset):
            subject = UNSET
        else:
            subject = self.subject

        thread_id: None | str | Unset
        if isinstance(self.thread_id, Unset):
            thread_id = UNSET
        else:
            thread_id = self.thread_id

        to_json: None | str | Unset
        if isinstance(self.to_json, Unset):
            to_json = UNSET
        else:
            to_json = self.to_json

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "folderId": folder_id,
                "hasAttachments": has_attachments,
                "id": id,
                "isDraft": is_draft,
                "isFlagged": is_flagged,
                "isRead": is_read,
                "logicalId": logical_id,
                "occurrenceId": occurrence_id,
                "uid": uid,
            }
        )
        if date is not UNSET:
            field_dict["date"] = date
        if from_email is not UNSET:
            field_dict["fromEmail"] = from_email
        if from_name is not UNSET:
            field_dict["fromName"] = from_name
        if message_id is not UNSET:
            field_dict["messageId"] = message_id
        if preview is not UNSET:
            field_dict["preview"] = preview
        if snoozed_until is not UNSET:
            field_dict["snoozedUntil"] = snoozed_until
        if subject is not UNSET:
            field_dict["subject"] = subject
        if thread_id is not UNSET:
            field_dict["threadId"] = thread_id
        if to_json is not UNSET:
            field_dict["toJson"] = to_json

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("accountId")

        folder_id = d.pop("folderId")

        has_attachments = d.pop("hasAttachments")

        id = d.pop("id")

        is_draft = d.pop("isDraft")

        is_flagged = d.pop("isFlagged")

        is_read = d.pop("isRead")

        logical_id = d.pop("logicalId")

        occurrence_id = d.pop("occurrenceId")

        uid = d.pop("uid")

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

        def _parse_message_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        message_id = _parse_message_id(d.pop("messageId", UNSET))

        def _parse_preview(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        preview = _parse_preview(d.pop("preview", UNSET))

        def _parse_snoozed_until(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        snoozed_until = _parse_snoozed_until(d.pop("snoozedUntil", UNSET))

        def _parse_subject(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        subject = _parse_subject(d.pop("subject", UNSET))

        def _parse_thread_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        thread_id = _parse_thread_id(d.pop("threadId", UNSET))

        def _parse_to_json(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        to_json = _parse_to_json(d.pop("toJson", UNSET))

        message = cls(
            account_id=account_id,
            folder_id=folder_id,
            has_attachments=has_attachments,
            id=id,
            is_draft=is_draft,
            is_flagged=is_flagged,
            is_read=is_read,
            logical_id=logical_id,
            occurrence_id=occurrence_id,
            uid=uid,
            date=date,
            from_email=from_email,
            from_name=from_name,
            message_id=message_id,
            preview=preview,
            snoozed_until=snoozed_until,
            subject=subject,
            thread_id=thread_id,
            to_json=to_json,
        )

        message.additional_properties = d
        return message

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
