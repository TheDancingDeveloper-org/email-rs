from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.cursor_messages_response import CursorMessagesResponse
from ...models.error_response import ErrorResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    kind: str,
    *,
    page: int | Unset = UNSET,
    per_page: int | Unset = UNSET,
    cursor: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["page"] = page

    params["per_page"] = per_page

    params["cursor"] = cursor

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/smart-folders/{kind}/messages".format(
            kind=quote(str(kind), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> CursorMessagesResponse | ErrorResponse | None:
    if response.status_code == 200:
        response_200 = CursorMessagesResponse.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = ErrorResponse.from_dict(response.json())

        return response_401

    if response.status_code == 403:
        response_403 = ErrorResponse.from_dict(response.json())

        return response_403

    if response.status_code == 404:
        response_404 = ErrorResponse.from_dict(response.json())

        return response_404

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[CursorMessagesResponse | ErrorResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    kind: str,
    *,
    client: AuthenticatedClient,
    page: int | Unset = UNSET,
    per_page: int | Unset = UNSET,
    cursor: str | Unset = UNSET,
) -> Response[CursorMessagesResponse | ErrorResponse]:
    """
    Args:
        kind (str):
        page (int | Unset):
        per_page (int | Unset):
        cursor (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CursorMessagesResponse | ErrorResponse]
    """

    kwargs = _get_kwargs(
        kind=kind,
        page=page,
        per_page=per_page,
        cursor=cursor,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    kind: str,
    *,
    client: AuthenticatedClient,
    page: int | Unset = UNSET,
    per_page: int | Unset = UNSET,
    cursor: str | Unset = UNSET,
) -> CursorMessagesResponse | ErrorResponse | None:
    """
    Args:
        kind (str):
        page (int | Unset):
        per_page (int | Unset):
        cursor (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CursorMessagesResponse | ErrorResponse
    """

    return sync_detailed(
        kind=kind,
        client=client,
        page=page,
        per_page=per_page,
        cursor=cursor,
    ).parsed


async def asyncio_detailed(
    kind: str,
    *,
    client: AuthenticatedClient,
    page: int | Unset = UNSET,
    per_page: int | Unset = UNSET,
    cursor: str | Unset = UNSET,
) -> Response[CursorMessagesResponse | ErrorResponse]:
    """
    Args:
        kind (str):
        page (int | Unset):
        per_page (int | Unset):
        cursor (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CursorMessagesResponse | ErrorResponse]
    """

    kwargs = _get_kwargs(
        kind=kind,
        page=page,
        per_page=per_page,
        cursor=cursor,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    kind: str,
    *,
    client: AuthenticatedClient,
    page: int | Unset = UNSET,
    per_page: int | Unset = UNSET,
    cursor: str | Unset = UNSET,
) -> CursorMessagesResponse | ErrorResponse | None:
    """
    Args:
        kind (str):
        page (int | Unset):
        per_page (int | Unset):
        cursor (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CursorMessagesResponse | ErrorResponse
    """

    return (
        await asyncio_detailed(
            kind=kind,
            client=client,
            page=page,
            per_page=per_page,
            cursor=cursor,
        )
    ).parsed
