from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CalendarSearchResult")


@_attrs_define
class CalendarSearchResult:
    """
    Attributes:
        end_at (str):
        id (str):
        is_all_day (bool):
        start_at (str):
        title (str):
        description (None | str | Unset):
        location (None | str | Unset):
    """

    end_at: str
    id: str
    is_all_day: bool
    start_at: str
    title: str
    description: None | str | Unset = UNSET
    location: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        end_at = self.end_at

        id = self.id

        is_all_day = self.is_all_day

        start_at = self.start_at

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

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
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

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        end_at = d.pop("endAt")

        id = d.pop("id")

        is_all_day = d.pop("isAllDay")

        start_at = d.pop("startAt")

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

        calendar_search_result = cls(
            end_at=end_at,
            id=id,
            is_all_day=is_all_day,
            start_at=start_at,
            title=title,
            description=description,
            location=location,
        )

        calendar_search_result.additional_properties = d
        return calendar_search_result

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
