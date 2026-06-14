#!/usr/bin/env python
# -*- coding: utf-8 -*-

from tortoise import fields
from tortoise.models import Model


class Song(Model):
    id = fields.IntField(pk=True, generated=True)
    display_name = fields.CharField(max_length=256)
    source_path = fields.CharField(max_length=1024, unique=True)
    source_origin = fields.CharField(max_length=16)  # scan | upload
    source_rel = fields.CharField(max_length=512, null=True)
    media_kind = fields.CharField(max_length=16, default='video')
    playback_mode = fields.CharField(max_length=16, default='plain')  # plain | enhanced
    playback_source = fields.CharField(max_length=16, null=True)  # override | embedded | plain
    can_queue = fields.BooleanField(null=True)
    is_playable = fields.BooleanField(default=False)
    scan_root = fields.CharField(max_length=1024, null=True)
    audio_layout = fields.TextField(null=True)
    create_time = fields.DatetimeField(auto_now_add=True)
    update_time = fields.DatetimeField(auto_now=True)

    class Meta:
        db_table = 'song'


class History(Model):
    id = fields.IntField(pk=True)  # 与 Song.id 一致
    name = fields.CharField(max_length=256)
    times = fields.IntField(default=0)
    is_sing = fields.IntField(default=0)  # QueueState: 0 待播 | 1 已唱 | -1 正在唱
    is_top = fields.IntField(default=0)
    create_time = fields.DatetimeField(auto_now_add=True)
    update_time = fields.DatetimeField(auto_now=True, index=True)

    class Meta:
        db_table = 'history'
