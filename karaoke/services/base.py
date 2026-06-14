#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""Service 层公共工具：减少 try/except 与分页样板。"""

from typing import Any, Awaitable, Callable, Optional, TypeVar

from tortoise.exceptions import DoesNotExist

from karaoke.dto.api_result import ApiResult
from karaoke.errors import fail_result

T = TypeVar('T')


def apply_pagination(result: ApiResult, total: int, page: int, page_size: int) -> ApiResult:
    return result.with_pagination(total, page, page_size)


async def run_guarded(
    action: str,
    fn: Callable[[], Awaitable[Any]],
    *,
    not_found_label: Optional[str] = None,
) -> ApiResult:
    """执行异步业务逻辑，统一捕获 DoesNotExist 与未知异常。"""
    result = ApiResult()
    try:
        payload = await fn()
        if isinstance(payload, ApiResult):
            return payload
        if payload is not None:
            result.data = payload
        return result
    except DoesNotExist:
        return ApiResult.not_found(not_found_label or '资源')
    except Exception as exc:
        fail_result(result, exc, action)
        return result
