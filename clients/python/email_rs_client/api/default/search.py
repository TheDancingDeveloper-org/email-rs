from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error_response import ErrorResponse
from ...models.search_response import SearchResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    q: str | Unset = UNSET,
    account_id: str | Unset = UNSET,
    match: str | Unset = UNSET,
    from_: str | Unset = UNSET,
    to: str | Unset = UNSET,
    cc: str | Unset = UNSET,
    subject: str | Unset = UNSET,
    body: str | Unset = UNSET,
    filename: str | Unset = UNSET,
    has_attachment: bool | Unset = UNSET,
    is_read: bool | Unset = UNSET,
    is_flagged: bool | Unset = UNSET,
    date_after: str | Unset = UNSET,
    date_before: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["q"] = q

    params["account_id"] = account_id

    params["match"] = match

    params["from"] = from_

    params["to"] = to

    params["cc"] = cc

    params["subject"] = subject

    params["body"] = body

    params["filename"] = filename

    params["has_attachment"] = has_attachment

    params["is_read"] = is_read

    params["is_flagged"] = is_flagged

    params["date_after"] = date_after

    params["date_before"] = date_before

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/search",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ErrorResponse | SearchResponse | None:
    if response.status_code == 200:
        response_200 = SearchResponse.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = ErrorResponse.from_dict(response.json())

        return response_401

    if response.status_code == 403:
        response_403 = ErrorResponse.from_dict(response.json())

        return response_403

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ErrorResponse | SearchResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    q: str | Unset = UNSET,
    account_id: str | Unset = UNSET,
    match: str | Unset = UNSET,
    from_: str | Unset = UNSET,
    to: str | Unset = UNSET,
    cc: str | Unset = UNSET,
    subject: str | Unset = UNSET,
    body: str | Unset = UNSET,
    filename: str | Unset = UNSET,
    has_attachment: bool | Unset = UNSET,
    is_read: bool | Unset = UNSET,
    is_flagged: bool | Unset = UNSET,
    date_after: str | Unset = UNSET,
    date_before: str | Unset = UNSET,
) -> Response[ErrorResponse | SearchResponse]:
    """
    Args:
        q (str | Unset):
        account_id (str | Unset):
        match (str | Unset):
        from_ (str | Unset):
        to (str | Unset):
        cc (str | Unset):
        subject (str | Unset):
        body (str | Unset):
        filename (str | Unset):
        has_attachment (bool | Unset):
        is_read (bool | Unset):
        is_flagged (bool | Unset):
        date_after (str | Unset):
        date_before (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ErrorResponse | SearchResponse]
    """

    kwargs = _get_kwargs(
        q=q,
        account_id=account_id,
        match=match,
        from_=from_,
        to=to,
        cc=cc,
        subject=subject,
        body=body,
        filename=filename,
        has_attachment=has_attachment,
        is_read=is_read,
        is_flagged=is_flagged,
        date_after=date_after,
        date_before=date_before,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient,
    q: str | Unset = UNSET,
    account_id: str | Unset = UNSET,
    match: str | Unset = UNSET,
    from_: str | Unset = UNSET,
    to: str | Unset = UNSET,
    cc: str | Unset = UNSET,
    subject: str | Unset = UNSET,
    body: str | Unset = UNSET,
    filename: str | Unset = UNSET,
    has_attachment: bool | Unset = UNSET,
    is_read: bool | Unset = UNSET,
    is_flagged: bool | Unset = UNSET,
    date_after: str | Unset = UNSET,
    date_before: str | Unset = UNSET,
) -> ErrorResponse | SearchResponse | None:
    """
    Args:
        q (str | Unset):
        account_id (str | Unset):
        match (str | Unset):
        from_ (str | Unset):
        to (str | Unset):
        cc (str | Unset):
        subject (str | Unset):
        body (str | Unset):
        filename (str | Unset):
        has_attachment (bool | Unset):
        is_read (bool | Unset):
        is_flagged (bool | Unset):
        date_after (str | Unset):
        date_before (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ErrorResponse | SearchResponse
    """

    return sync_detailed(
        client=client,
        q=q,
        account_id=account_id,
        match=match,
        from_=from_,
        to=to,
        cc=cc,
        subject=subject,
        body=body,
        filename=filename,
        has_attachment=has_attachment,
        is_read=is_read,
        is_flagged=is_flagged,
        date_after=date_after,
        date_before=date_before,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    q: str | Unset = UNSET,
    account_id: str | Unset = UNSET,
    match: str | Unset = UNSET,
    from_: str | Unset = UNSET,
    to: str | Unset = UNSET,
    cc: str | Unset = UNSET,
    subject: str | Unset = UNSET,
    body: str | Unset = UNSET,
    filename: str | Unset = UNSET,
    has_attachment: bool | Unset = UNSET,
    is_read: bool | Unset = UNSET,
    is_flagged: bool | Unset = UNSET,
    date_after: str | Unset = UNSET,
    date_before: str | Unset = UNSET,
) -> Response[ErrorResponse | SearchResponse]:
    """
    Args:
        q (str | Unset):
        account_id (str | Unset):
        match (str | Unset):
        from_ (str | Unset):
        to (str | Unset):
        cc (str | Unset):
        subject (str | Unset):
        body (str | Unset):
        filename (str | Unset):
        has_attachment (bool | Unset):
        is_read (bool | Unset):
        is_flagged (bool | Unset):
        date_after (str | Unset):
        date_before (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ErrorResponse | SearchResponse]
    """

    kwargs = _get_kwargs(
        q=q,
        account_id=account_id,
        match=match,
        from_=from_,
        to=to,
        cc=cc,
        subject=subject,
        body=body,
        filename=filename,
        has_attachment=has_attachment,
        is_read=is_read,
        is_flagged=is_flagged,
        date_after=date_after,
        date_before=date_before,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient,
    q: str | Unset = UNSET,
    account_id: str | Unset = UNSET,
    match: str | Unset = UNSET,
    from_: str | Unset = UNSET,
    to: str | Unset = UNSET,
    cc: str | Unset = UNSET,
    subject: str | Unset = UNSET,
    body: str | Unset = UNSET,
    filename: str | Unset = UNSET,
    has_attachment: bool | Unset = UNSET,
    is_read: bool | Unset = UNSET,
    is_flagged: bool | Unset = UNSET,
    date_after: str | Unset = UNSET,
    date_before: str | Unset = UNSET,
) -> ErrorResponse | SearchResponse | None:
    """
    Args:
        q (str | Unset):
        account_id (str | Unset):
        match (str | Unset):
        from_ (str | Unset):
        to (str | Unset):
        cc (str | Unset):
        subject (str | Unset):
        body (str | Unset):
        filename (str | Unset):
        has_attachment (bool | Unset):
        is_read (bool | Unset):
        is_flagged (bool | Unset):
        date_after (str | Unset):
        date_before (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ErrorResponse | SearchResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            q=q,
            account_id=account_id,
            match=match,
            from_=from_,
            to=to,
            cc=cc,
            subject=subject,
            body=body,
            filename=filename,
            has_attachment=has_attachment,
            is_read=is_read,
            is_flagged=is_flagged,
            date_after=date_after,
            date_before=date_before,
        )
    ).parsed
