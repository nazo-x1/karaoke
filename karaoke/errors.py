#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""将异常转为可读的 API 错误信息，完整堆栈仅写入日志。"""

import sqlite3
import traceback

from settings import logger


def format_api_error(exc: Exception, action: str = "操作失败") -> str:
    logger.error("%s\n%s", action, traceback.format_exc())

    if isinstance(exc, ValueError):
        text = str(exc).strip()
        return text or action

    if isinstance(exc, TypeError):
        text = str(exc).strip().lower()
        if "operand type" in text or "unsupported operand" in text:
            return f"{action}：请求参数格式不正确"
        return f"{action}：参数类型错误"

    if isinstance(exc, sqlite3.OperationalError):
        text = str(exc).strip().lower()
        if "no such column" in text:
            return f"{action}：数据库字段不完整，请重启服务以自动迁移"
        if "no such table" in text:
            return f"{action}：数据库表缺失，请重启服务初始化"
        return f"{action}：数据库访问异常"

    if isinstance(exc, (OSError, PermissionError)):
        text = str(exc).strip()
        if text:
            return f"{action}：{text[:160]}"
        return f"{action}：文件访问失败"

    text = str(exc).strip()
    if text and len(text) <= 160 and "\n" not in text:
        return f"{action}：{text}"

    return f"{action}（{exc.__class__.__name__}）"


def fail_result(result, exc: Exception, action: str) -> None:
    result.code = 1
    result.msg = format_api_error(exc, action)
