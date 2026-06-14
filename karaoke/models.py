#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Author: leeyoshinari

from tortoise import fields
from tortoise.models import Model
from pydantic import BaseModel


class Song(Model):
    id = fields.IntField(pk=True, generated=True, description='主键')
    display_name = fields.CharField(max_length=256, description='展示名')
    source_path = fields.CharField(max_length=1024, unique=True, description='源文件绝对路径')
    source_origin = fields.CharField(max_length=16, description='scan | upload')
    source_rel = fields.CharField(max_length=512, null=True, description='相对扫描根路径')
    media_kind = fields.CharField(max_length=16, default='video', description='媒体类型')
    playback_mode = fields.CharField(max_length=16, default='plain', description='plain | enhanced')
    is_playable = fields.BooleanField(default=False, description='ffprobe 验证是否可播放')
    scan_root = fields.CharField(max_length=1024, null=True, description='最近一次扫描根')
    create_time = fields.DatetimeField(auto_now_add=True)
    update_time = fields.DatetimeField(auto_now=True)

    class Meta:
        db_table = 'song'


class History(Model):
    id = fields.IntField(pk=True, description='主键，与 Song.id 一致')
    name = fields.CharField(max_length=256, description='展示名')
    times = fields.IntField(default=0, description='K歌次数')
    is_sing = fields.IntField(default=0, description='歌是否唱过, 0-没唱过, 1-唱过, -1-正在唱')
    is_top = fields.IntField(default=0, description='是否置顶, 0-不置顶, 1-置顶')
    create_time = fields.DatetimeField(auto_now_add=True)
    update_time = fields.DatetimeField(auto_now=True, index=True)

    class Meta:
        db_table = 'history'


class SongList(BaseModel):
    id: int
    name: str
    display_name: str
    source_origin: str
    playback_mode: str
    can_queue: bool
    is_playable: bool
    source_path: str
    create_time: str
    update_time: str

    class Config:
        from_attributes = True


class HistoryList(BaseModel):
    id: int
    name: str
    times: int
    is_sing: int
    is_top: int
    playback_mode: str = 'plain'

    class Config:
        from_attributes = True
