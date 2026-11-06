from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.list_unsubscribe import ListUnsubscribe
    from ..models.message_attachment import MessageAttachment
    from ..models.message_authentication_results import MessageAuthenticationResults
    from ..models.message_body import MessageBody


T = TypeVar("T", bound="FullMessage")


@_attrs_define
class FullMessage:
    """
    Attributes:
        account_id (str):
        folder_id (str):
        has_attachments (bool):
        id (str): Backward-compatible mailbox-occurrence identifier.
        is_draft (bool):
        is_flagged (bool):
        is_read (bool):
        logical_id (str):
        occurrence_id (str):
        uid (int):
        attachments (list[MessageAttachment] | Unset):
        authentication_results (MessageAuthenticationResults | Unset):
        body (MessageBody | None | Unset):
        date (None | str | Unset):
        display_name_spoofing (bool | Unset):
        from_email (None | str | Unset):
        from_name (None | str | Unset):
        from_reply_to_domain_mismatch (bool | Unset):
        list_unsubscribe (ListUnsubscribe | None | Unset):
        message_id (None | str | Unset):
        preview (None | str | Unset):
        subject (None | str | Unset):
        thread_id (None | str | Unset):
        to (None | str | Unset):
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
    attachments: list[MessageAttachment] | Unset = UNSET
    authentication_results: MessageAuthenticationResults | Unset = UNSET
    body: MessageBody | None | Unset = UNSET
    date: None | str | Unset = UNSET
    display_name_spoofing: bool | Unset = UNSET
    from_email: None | str | Unset = UNSET
    from_name: None | str | Unset = UNSET
    from_reply_to_domain_mismatch: bool | Unset = UNSET
    list_unsubscribe: ListUnsubscribe | None | Unset = UNSET
    message_id: None | str | Unset = UNSET
    preview: None | str | Unset = UNSET
    subject: None | str | Unset = UNSET
    thread_id: None | str | Unset = UNSET
    to: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.list_unsubscribe import ListUnsubscribe  # noqa: PLC0415
        from ..models.message_body import MessageBody  # noqa: PLC0415

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

        attachments: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.attachments, Unset):
            attachments = []
            for attachments_item_data in self.attachments:
                attachments_item = attachments_item_data.to_dict()
                attachments.append(attachments_item)

        authentication_results: dict[str, Any] | Unset = UNSET
        if not isinstance(self.authentication_results, Unset):
            authentication_results = self.authentication_results.to_dict()

        body: dict[str, Any] | None | Unset
        if isinstance(self.body, Unset):
            body = UNSET
        elif isinstance(self.body, MessageBody):
            body = self.body.to_dict()
        else:
            body = self.body

        date: None | str | Unset
        if isinstance(self.date, Unset):
            date = UNSET
        else:
            date = self.date

        display_name_spoofing = self.display_name_spoofing

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

        from_reply_to_domain_mismatch = self.from_reply_to_domain_mismatch

        list_unsubscribe: dict[str, Any] | None | Unset
        if isinstance(self.list_unsubscribe, Unset):
            list_unsubscribe = UNSET
        elif isinstance(self.list_unsubscribe, ListUnsubscribe):
            list_unsubscribe = self.list_unsubscribe.to_dict()
        else:
            list_unsubscribe = self.list_unsubscribe

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

        to: None | str | Unset
        if isinstance(self.to, Unset):
            to = UNSET
        else:
            to = self.to

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
        if attachments is not UNSET:
            field_dict["attachments"] = attachments
        if authentication_results is not UNSET:
            field_dict["authenticationResults"] = authentication_results
        if body is not UNSET:
            field_dict["body"] = body
        if date is not UNSET:
            field_dict["date"] = date
        if display_name_spoofing is not UNSET:
            field_dict["displayNameSpoofing"] = display_name_spoofing
        if from_email is not UNSET:
            field_dict["fromEmail"] = from_email
        if from_name is not UNSET:
            field_dict["fromName"] = from_name
        if from_reply_to_domain_mismatch is not UNSET:
            field_dict["fromReplyToDomainMismatch"] = from_reply_to_domain_mismatch
        if list_unsubscribe is not UNSET:
            field_dict["listUnsubscribe"] = list_unsubscribe
        if message_id is not UNSET:
            field_dict["messageId"] = message_id
        if preview is not UNSET:
            field_dict["preview"] = preview
        if subject is not UNSET:
            field_dict["subject"] = subject
        if thread_id is not UNSET:
            field_dict["threadId"] = thread_id
        if to is not UNSET:
            field_dict["to"] = to

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.list_unsubscribe import ListUnsubscribe  # noqa: PLC0415
        from ..models.message_attachment import MessageAttachment  # noqa: PLC0415
        from ..models.message_authentication_results import MessageAuthenticationResults  # noqa: PLC0415
        from ..models.message_body import MessageBody  # noqa: PLC0415

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

        _attachments = d.pop("attachments", UNSET)
        attachments: list[MessageAttachment] | Unset = UNSET
        if _attachments is not UNSET:
            attachments = []
            for attachments_item_data in _attachments:
                attachments_item = MessageAttachment.from_dict(attachments_item_data)

                attachments.append(attachments_item)

        _authentication_results = d.pop("authenticationResults", UNSET)
        authentication_results: MessageAuthenticationResults | Unset
        if isinstance(_authentication_results, Unset):
            authentication_results = UNSET
        else:
            authentication_results = MessageAuthenticationResults.from_dict(_authentication_results)

        def _parse_body(data: object) -> MessageBody | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                body_type_1 = MessageBody.from_dict(data)

                return body_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(MessageBody | None | Unset, data)

        body = _parse_body(d.pop("body", UNSET))

        def _parse_date(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        date = _parse_date(d.pop("date", UNSET))

        display_name_spoofing = d.pop("displayNameSpoofing", UNSET)

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

        from_reply_to_domain_mismatch = d.pop("fromReplyToDomainMismatch", UNSET)

        def _parse_list_unsubscribe(data: object) -> ListUnsubscribe | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                list_unsubscribe_type_1 = ListUnsubscribe.from_dict(data)

                return list_unsubscribe_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ListUnsubscribe | None | Unset, data)

        list_unsubscribe = _parse_list_unsubscribe(d.pop("listUnsubscribe", UNSET))

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

        def _parse_to(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        to = _parse_to(d.pop("to", UNSET))

        full_message = cls(
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
            attachments=attachments,
            authentication_results=authentication_results,
            body=body,
            date=date,
            display_name_spoofing=display_name_spoofing,
            from_email=from_email,
            from_name=from_name,
            from_reply_to_domain_mismatch=from_reply_to_domain_mismatch,
            list_unsubscribe=list_unsubscribe,
            message_id=message_id,
            preview=preview,
            subject=subject,
            thread_id=thread_id,
            to=to,
        )

        full_message.additional_properties = d
        return full_message

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
