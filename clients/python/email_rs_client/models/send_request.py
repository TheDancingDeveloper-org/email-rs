from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.outgoing_attachment import OutgoingAttachment


T = TypeVar("T", bound="SendRequest")


@_attrs_define
class SendRequest:
    """
    Attributes:
        account_id (str):
        bcc (list[str]):
        cc (list[str]):
        subject (str):
        to (list[str]):
        attachments (list[OutgoingAttachment] | Unset):
        html_body (None | str | Unset):
        in_reply_to (None | str | Unset):
        text_body (None | str | Unset):
    """

    account_id: str
    bcc: list[str]
    cc: list[str]
    subject: str
    to: list[str]
    attachments: list[OutgoingAttachment] | Unset = UNSET
    html_body: None | str | Unset = UNSET
    in_reply_to: None | str | Unset = UNSET
    text_body: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        bcc = self.bcc

        cc = self.cc

        subject = self.subject

        to = self.to

        attachments: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.attachments, Unset):
            attachments = []
            for attachments_item_data in self.attachments:
                attachments_item = attachments_item_data.to_dict()
                attachments.append(attachments_item)

        html_body: None | str | Unset
        if isinstance(self.html_body, Unset):
            html_body = UNSET
        else:
            html_body = self.html_body

        in_reply_to: None | str | Unset
        if isinstance(self.in_reply_to, Unset):
            in_reply_to = UNSET
        else:
            in_reply_to = self.in_reply_to

        text_body: None | str | Unset
        if isinstance(self.text_body, Unset):
            text_body = UNSET
        else:
            text_body = self.text_body

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accountId": account_id,
                "bcc": bcc,
                "cc": cc,
                "subject": subject,
                "to": to,
            }
        )
        if attachments is not UNSET:
            field_dict["attachments"] = attachments
        if html_body is not UNSET:
            field_dict["htmlBody"] = html_body
        if in_reply_to is not UNSET:
            field_dict["inReplyTo"] = in_reply_to
        if text_body is not UNSET:
            field_dict["textBody"] = text_body

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.outgoing_attachment import OutgoingAttachment

        d = dict(src_dict)
        account_id = d.pop("accountId")

        bcc = cast(list[str], d.pop("bcc"))

        cc = cast(list[str], d.pop("cc"))

        subject = d.pop("subject")

        to = cast(list[str], d.pop("to"))

        _attachments = d.pop("attachments", UNSET)
        attachments: list[OutgoingAttachment] | Unset = UNSET
        if _attachments is not UNSET:
            attachments = []
            for attachments_item_data in _attachments:
                attachments_item = OutgoingAttachment.from_dict(attachments_item_data)

                attachments.append(attachments_item)

        def _parse_html_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        html_body = _parse_html_body(d.pop("htmlBody", UNSET))

        def _parse_in_reply_to(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        in_reply_to = _parse_in_reply_to(d.pop("inReplyTo", UNSET))

        def _parse_text_body(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        text_body = _parse_text_body(d.pop("textBody", UNSET))

        send_request = cls(
            account_id=account_id,
            bcc=bcc,
            cc=cc,
            subject=subject,
            to=to,
            attachments=attachments,
            html_body=html_body,
            in_reply_to=in_reply_to,
            text_body=text_body,
        )

        send_request.additional_properties = d
        return send_request

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
