from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.account_candidate import AccountCandidate


T = TypeVar("T", bound="DiscoveryResponse")


@_attrs_define
class DiscoveryResponse:
    """
    Attributes:
        candidates (list[AccountCandidate]):
        domain (str):
        email (str):
    """

    candidates: list[AccountCandidate]
    domain: str
    email: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        candidates = []
        for candidates_item_data in self.candidates:
            candidates_item = candidates_item_data.to_dict()
            candidates.append(candidates_item)

        domain = self.domain

        email = self.email

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "candidates": candidates,
                "domain": domain,
                "email": email,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.account_candidate import AccountCandidate  # noqa: PLC0415

        d = dict(src_dict)
        candidates = []
        _candidates = d.pop("candidates")
        for candidates_item_data in _candidates:
            candidates_item = AccountCandidate.from_dict(candidates_item_data)

            candidates.append(candidates_item)

        domain = d.pop("domain")

        email = d.pop("email")

        discovery_response = cls(
            candidates=candidates,
            domain=domain,
            email=email,
        )

        discovery_response.additional_properties = d
        return discovery_response

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
