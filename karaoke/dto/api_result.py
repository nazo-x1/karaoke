#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""统一 API 响应模型（/api/v1 JSON  envelope）。"""

from typing import Any, Optional


class ApiResult:
    """与前端约定：code=0 成功，data 承载业务载荷，分页字段 total/page/totalPage。"""

    __slots__ = ('code', 'msg', 'data', 'total', 'page', 'totalPage')

    def __init__(
        self,
        code: int = 0,
        msg: str = 'Success!',
        data: Any = None,
        total: int = 0,
        page: int = 0,
        total_page: int = 0,
    ) -> None:
        self.code = code
        self.msg = msg
        self.data = data
        self.total = total
        self.page = page
        self.totalPage = total_page

    @classmethod
    def ok(cls, data: Any = None, msg: str = 'Success!', **meta: Any) -> 'ApiResult':
        return cls(data=data, msg=msg, **meta)

    @classmethod
    def fail(cls, msg: str, code: int = 1, data: Any = None) -> 'ApiResult':
        return cls(code=code, msg=msg, data=data)

    @classmethod
    def not_found(cls, label: str = '资源') -> 'ApiResult':
        return cls.fail(f'{label}不存在')

    def with_pagination(self, total: int, page: int, page_size: int) -> 'ApiResult':
        self.total = total
        self.page = page
        self.totalPage = (total + page_size - 1) // page_size if total else 0
        return self

    def to_dict(self) -> dict:
        return {
            'code': self.code,
            'msg': self.msg,
            'data': self.data,
            'total': self.total,
            'page': self.page,
            'totalPage': self.totalPage,
        }
