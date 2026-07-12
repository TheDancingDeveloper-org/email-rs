from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreateAccount")


@_attrs_define
class CreateAccount:
    """
    Attributes:
        auth_type (str):
        email (str):
        name (str):
        provider_type (str):
        host (None | str | Unset):
        oauth_token_json (None | str | Unset):
        password (None | str | Unset):
        port (int | None | Unset):
        smtp_host (None | str | Unset):
        smtp_password (None | str | Unset):
        smtp_port (int | None | Unset):
        use_ssl (bool | None | Unset):
    """

    auth_type: str
    email: str
    name: str
    provider_type: str
    host: None | str | Unset = UNSET
    oauth_token_json: None | str | Unset = UNSET
    password: None | str | Unset = UNSET
    port: int | None | Unset = UNSET
    smtp_host: None | str | Unset = UNSET
    smtp_password: None | str | Unset = UNSET
    smtp_port: int | None | Unset = UNSET
    use_ssl: bool | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        auth_type = self.auth_type

        email = self.email

        name = self.name

        provider_type = self.provider_type

        host: None | str | Unset
        if isinstance(self.host, Unset):
            host = UNSET
        else:
            host = self.host

        oauth_token_json: None | str | Unset
        if isinstance(self.oauth_token_json, Unset):
            oauth_token_json = UNSET
        else:
            oauth_token_json = self.oauth_token_json

        password: None | str | Unset
        if isinstance(self.password, Unset):
            password = UNSET
        else:
            password = self.password

        port: int | None | Unset
        if isinstance(self.port, Unset):
            port = UNSET
        else:
            port = self.port

        smtp_host: None | str | Unset
        if isinstance(self.smtp_host, Unset):
            smtp_host = UNSET
        else:
            smtp_host = self.smtp_host

        smtp_password: None | str | Unset
        if isinstance(self.smtp_password, Unset):
            smtp_password = UNSET
        else:
            smtp_password = self.smtp_password

        smtp_port: int | None | Unset
        if isinstance(self.smtp_port, Unset):
            smtp_port = UNSET
        else:
            smtp_port = self.smtp_port

        use_ssl: bool | None | Unset
        if isinstance(self.use_ssl, Unset):
            use_ssl = UNSET
        else:
            use_ssl = self.use_ssl

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "authType": auth_type,
                "email": email,
                "name": name,
                "providerType": provider_type,
            }
        )
        if host is not UNSET:
            field_dict["host"] = host
        if oauth_token_json is not UNSET:
            field_dict["oauthTokenJson"] = oauth_token_json
        if password is not UNSET:
            field_dict["password"] = password
        if port is not UNSET:
            field_dict["port"] = port
        if smtp_host is not UNSET:
            field_dict["smtpHost"] = smtp_host
        if smtp_password is not UNSET:
            field_dict["smtpPassword"] = smtp_password
        if smtp_port is not UNSET:
            field_dict["smtpPort"] = smtp_port
        if use_ssl is not UNSET:
            field_dict["useSsl"] = use_ssl

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        auth_type = d.pop("authType")

        email = d.pop("email")

        name = d.pop("name")

        provider_type = d.pop("providerType")

        def _parse_host(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        host = _parse_host(d.pop("host", UNSET))

        def _parse_oauth_token_json(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        oauth_token_json = _parse_oauth_token_json(d.pop("oauthTokenJson", UNSET))

        def _parse_password(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        password = _parse_password(d.pop("password", UNSET))

        def _parse_port(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        port = _parse_port(d.pop("port", UNSET))

        def _parse_smtp_host(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        smtp_host = _parse_smtp_host(d.pop("smtpHost", UNSET))

        def _parse_smtp_password(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        smtp_password = _parse_smtp_password(d.pop("smtpPassword", UNSET))

        def _parse_smtp_port(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        smtp_port = _parse_smtp_port(d.pop("smtpPort", UNSET))

        def _parse_use_ssl(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        use_ssl = _parse_use_ssl(d.pop("useSsl", UNSET))

        create_account = cls(
            auth_type=auth_type,
            email=email,
            name=name,
            provider_type=provider_type,
            host=host,
            oauth_token_json=oauth_token_json,
            password=password,
            port=port,
            smtp_host=smtp_host,
            smtp_password=smtp_password,
            smtp_port=smtp_port,
            use_ssl=use_ssl,
        )

        create_account.additional_properties = d
        return create_account

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
