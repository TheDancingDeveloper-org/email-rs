from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="CapabilitiesResponse")


@_attrs_define
class CapabilitiesResponse:
    """
    Attributes:
        api_version (str):
        capabilities (list[str]):
        integration_scopes (list[str]):
        product_version (str):
    """

    api_version: str
    capabilities: list[str]
    integration_scopes: list[str]
    product_version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        api_version = self.api_version

        capabilities = self.capabilities

        integration_scopes = self.integration_scopes

        product_version = self.product_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "apiVersion": api_version,
                "capabilities": capabilities,
                "integrationScopes": integration_scopes,
                "productVersion": product_version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        api_version = d.pop("apiVersion")

        capabilities = cast(list[str], d.pop("capabilities"))

        integration_scopes = cast(list[str], d.pop("integrationScopes"))

        product_version = d.pop("productVersion")

        capabilities_response = cls(
            api_version=api_version,
            capabilities=capabilities,
            integration_scopes=integration_scopes,
            product_version=product_version,
        )

        capabilities_response.additional_properties = d
        return capabilities_response

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
