#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""路由层依赖：序列化 Service 返回的 ApiResult。"""

from karaoke.dto.api_result import ApiResult


def as_json(result: ApiResult) -> dict:
    """显式转为 JSON dict（FastAPI 默认也能序列化 ApiResult 属性）。"""
    return result.to_dict()
